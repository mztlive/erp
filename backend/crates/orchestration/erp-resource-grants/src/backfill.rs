//! 存量证据回填只追加授权事件，不重放审批、转派或财务命令。
use std::collections::BTreeSet;

use application_core::AuditActor;
use bpm::model::types::ApprovalNodeExecutionStatus;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_finance::repository::extensions::ReceivableExt;
use erp_identity::entity::resource_access::event::{GrantEvent, GrantEventEvidence, GrantEventPayload};
use erp_identity::service::resource_access::append_event;
use erp_identity::{Error, Result};
use erp_workflow::entity::work_item_command::WorkItemCommandResult;
pub use erp_workflow::repository::grant_history::GrantHistoryKind;
use erp_workflow::repository::grant_history::{self, GrantHistoryEntry};
use erp_workflow::repository::{BpmExt, WorkItemExt};
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use crate::ResourceGrantProcess;
use crate::facts::{Facts, validate_targets};
use crate::management::validate_cursor;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackfillRequest {
    pub kind: GrantHistoryKind,
    pub after: Option<String>,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}
fn default_dry_run() -> bool {
    true
}

#[derive(Debug, Default, Serialize)]
pub struct BackfillReport {
    pub scanned: usize,
    pub eligible: usize,
    pub appended: usize,
    pub existing: usize,
    pub terminated: usize,
    pub invalid: Vec<BackfillProblem>,
    pub next_cursor: Option<String>,
    pub dry_run: bool,
}
#[derive(Debug, Serialize)]
pub struct BackfillProblem {
    pub record_id: String,
    pub reason: String,
}

impl ResourceGrantProcess {
    /// 按正式证据预览或执行一页存量回填。
    /// # 参数
    /// actor 为管理员，request 固定证据类型、排他游标及 dry_run。
    /// # 返回
    /// 返回新增、已有、永久终止和无效证据明细；空页结束本类型扫描。
    /// # 错误
    /// 权限或基础设施错误中止整页；未知提交结果不自动重试。
    pub async fn backfill(&self, actor: &AuditActor, request: BackfillRequest) -> Result<BackfillReport> {
        validate_cursor(request.after.as_deref())?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |ex| {
                Box::pin(async move {
                    this.management().authorize_manager(&actor, ex).await?;
                    this.backfill_page(request, ex).await
                })
            })
            .await
    }

    async fn backfill_page(&self, request: BackfillRequest, ex: &mut dyn Executor) -> Result<BackfillReport> {
        let entries = grant_history::page(&self.db, request.kind, request.after.as_deref(), ex).await?;
        let mut report = BackfillReport {
            scanned: entries.len(),
            next_cursor: entries.last().map(|row| row.id().to_owned()),
            dry_run: request.dry_run,
            ..Default::default()
        };
        let mut planned = BTreeSet::new();
        for entry in entries {
            let prepared = match self.prepare_history(&entry, &planned, ex).await {
                Ok(value) => value,
                Err(Error::ConflictError(reason) | Error::ValidationError(reason)) => {
                    report.invalid.push(BackfillProblem { record_id: entry.id().into(), reason });
                    continue;
                },
                Err(error) => return Err(error),
            };
            report.existing += prepared.existing;
            report.terminated += prepared.terminated;
            report.eligible += prepared.events.len();
            for event in prepared.events {
                planned.insert(event.source.id()?);
                if !report.dry_run && append_event(&self.db, event.evidence, ex).await? {
                    report.appended += 1;
                }
            }
        }
        Ok(report)
    }

    async fn prepare_history(
        &self,
        entry: &GrantHistoryEntry,
        planned: &BTreeSet<String>,
        ex: &mut dyn Executor,
    ) -> Result<PreparedHistory> {
        let events = self.historical_events(entry, ex).await?;
        let mut prepared = PreparedHistory::default();
        for evidence in events {
            let event = GrantEvent::new(evidence)?;
            if self.queue.source(&event.source, ex).await?.is_some_and(|source| source.terminated) {
                prepared.terminated += 1;
                continue;
            }
            let update = Facts::compute(&self.db, &event, ex).await?;
            validate_targets(&self.db, &update.targets, ex).await?;
            if planned.contains(&event.source.id()?)
                || self.queue.source_event(&event.source, ex).await?.is_some()
            {
                prepared.existing += 1;
                continue;
            }
            prepared.events.push(event);
        }
        Ok(prepared)
    }

    async fn historical_events(
        &self,
        entry: &GrantHistoryEntry,
        ex: &mut dyn Executor,
    ) -> Result<Vec<GrantEventEvidence>> {
        match entry {
            GrantHistoryEntry::Invoice(approval) => {
                let request = self
                    .db
                    .sales_invoice_requests()
                    .find_by_id(approval.subject.subject_id(), ex)
                    .await?
                    .ok_or_else(invalid)?;
                let task_id = request.work_item_id.as_deref().ok_or_else(invalid)?;
                let task = self.db.work_items().find_by_id(task_id, ex).await?.ok_or_else(invalid)?;
                let recipient = task.responsibility_actor_ids.first().ok_or_else(invalid)?.clone();
                Ok(vec![GrantEventEvidence {
                    payload: GrantEventPayload::InvoiceApproved {
                        invoice_request_id: request.base.id.clone(),
                        approval_instance_id: approval.base.id.clone(),
                        work_item_id: task.base.id,
                    },
                    recipient,
                    fact_version: request.base.version,
                    occurred_at: request.base.updated_at,
                }])
            },
            GrantHistoryEntry::Assignment(execution) => {
                if execution.status == ApprovalNodeExecutionStatus::Blocked {
                    return Ok(Vec::new());
                }
                let instance = self
                    .db
                    .approval_process_instances()
                    .find_by_id(execution.process_instance_id.as_ref(), ex)
                    .await?
                    .ok_or_else(invalid)?;
                let mut events = Vec::new();
                for event in assignment_events(&instance, execution) {
                    if grant_history::assigned_to(&self.db, &execution.base.id, &event.recipient, ex).await? {
                        events.push(event);
                    }
                }
                Ok(events)
            },
            GrantHistoryEntry::Reassignment(receipt) => {
                let WorkItemCommandResult::Reassigned { work_item_id, .. } = &receipt.result else {
                    return Err(invalid());
                };
                let task = self.db.work_items().find_by_id(work_item_id, ex).await?.ok_or_else(invalid)?;
                Ok(receipt.invoice_grant(&task).map_err(|_| invalid())?.into_iter().collect())
            },
        }
    }
}

#[derive(Default)]
struct PreparedHistory {
    events: Vec<GrantEvent>,
    existing: usize,
    terminated: usize,
}

fn invalid() -> Error {
    Error::ConflictError("历史审批、任务或正式分配证据缺失或错配".into())
}

fn assignment_events(
    instance: &ApprovalProcessInstance,
    execution: &ApprovalNodeExecution,
) -> Vec<GrantEventEvidence> {
    let payload = match instance.subject.subject_kind() {
        "sales_order" => GrantEventPayload::SalesOrderApprovalAssigned {
            sales_order_id: instance.subject.subject_id().into(),
            approval_instance_id: instance.base.id.clone(),
            node_id: execution.node_key.clone(),
            assignment_id: execution.base.id.clone(),
        },
        "purchase_order" => GrantEventPayload::PurchaseOrderApprovalAssigned {
            purchase_order_id: instance.subject.subject_id().into(),
            approval_instance_id: instance.base.id.clone(),
            node_id: execution.node_key.clone(),
            assignment_id: execution.base.id.clone(),
        },
        _ => return Vec::new(),
    };
    let recipients = std::iter::once(execution.assignee_participant_id.as_str())
        .chain(execution.additional_assignees.iter().map(|assignee| assignee.participant_id.as_str()))
        .collect::<BTreeSet<_>>();
    recipients
        .into_iter()
        .map(|recipient| GrantEventEvidence {
            payload: payload.clone(),
            recipient: recipient.into(),
            fact_version: instance.base.version,
            occurred_at: execution.base.created_at,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        ApprovalParticipant, NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef,
        Timestamp,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn backfill_defaults_to_preview_and_rejects_unknown_fields() {
        let request: BackfillRequest =
            serde_json::from_value(json!({"kind":"invoice_reassignments"})).unwrap();
        assert!(request.dry_run);
        let request: BackfillRequest =
            serde_json::from_value(json!({"kind":"invoice_reassignments", "dry_run":false})).unwrap();
        assert!(!request.dry_run);
        assert!(
            serde_json::from_value::<BackfillRequest>(json!({
                "kind":"invoice_reassignments", "recipient":"forged"
            }))
            .is_err()
        );
    }

    #[test]
    fn historical_orders_use_frozen_recipients_and_distinct_sources() {
        for (kind, process) in
            [("sales_order", ProcessKind::SalesOrder), ("purchase_order", ProcessKind::PurchaseOrder)]
        {
            let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
                id: ApprovalProcessInstanceId::new("instance"),
                process_definition_id: ApprovalProcessDefinitionId::new("definition"),
                definition_version: 1,
                process_kind: process,
                subject: SubjectRef::new(kind, "S001").unwrap(),
                subject_version: 1,
                started_by: ParticipantId::new("applicant").unwrap(),
                at: Timestamp::from_unix_secs(10).unwrap(),
            })
            .unwrap();
            instance.base.version = 7;
            let mut execution = ApprovalNodeExecution::new_active(NewNodeExecution {
                id: ApprovalNodeExecutionId::new("node"),
                process_instance_id: ApprovalProcessInstanceId::new("instance"),
                node_key: "review".into(),
                node_name: "审批".into(),
                round_no: 1,
                execution_no: 1,
                assignment_source: ApprovalExecutionAssignmentSource::Definition,
                replaces_execution_id: None,
                assignee_participant_id: ParticipantId::new("first").unwrap(),
                assignee_name_snapshot: "审批人".into(),
                at: Timestamp::from_unix_secs(11).unwrap(),
            })
            .unwrap();
            execution.additional_assignees.push(ApprovalParticipant {
                participant_id: ParticipantId::new("second").unwrap(),
                name: "另一审批人".into(),
            });
            let events = assignment_events(&instance, &execution);
            assert_eq!(
                events.iter().map(|event| event.recipient.as_str()).collect::<Vec<_>>(),
                vec!["first", "second"]
            );
            let first = GrantEvent::new(events[0].clone()).unwrap();
            let second = GrantEvent::new(events[1].clone()).unwrap();
            assert_ne!(first.source, second.source);
            assert_eq!(first.evidence.fact_version, 7);
            assert_eq!(first.evidence.occurred_at, 11);
            first.ensure_replay(&GrantEvent::new(events[0].clone()).unwrap()).unwrap();
            instance.subject = SubjectRef::new("unrelated", "S001").unwrap();
            assert!(assignment_events(&instance, &execution).is_empty());
        }
    }
}
