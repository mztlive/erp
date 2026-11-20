//! 正式审批与转派证据的有界回填读取，不读取当前候选人。
use std::collections::BTreeSet;

use bpm::model::types::ApprovalNodeExecutionStatus;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_identity::entity::resource_access::event::GrantEventEvidence;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::authorization::grant_facts::assignment_evidence;
use crate::entity::work_item_command::WorkItemCommandReceipt;
use crate::repository::{BpmExt, WorkItemCommandExt, WorkItemExt};

/// 每页固定最多 25 条正式证据，调用方按末尾主键续页。
pub const HISTORY_PAGE_SIZE: i64 = 25;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantHistoryKind {
    InvoiceApprovals,
    OrderAssignments,
    InvoiceReassignments,
}

/// 授权回填读取的历史记录；保留原始游标及已投影的业务证据。
pub enum GrantHistoryEntry {
    Invoice {
        id: String,
        invoice_request_id: String,
    },
    Assignment {
        id: String,
        events: Vec<GrantEventEvidence>,
    },
    /// 缺少有效审批实例的节点仍占用一条分页记录，由调用方报告为无效。
    InvalidAssignment {
        id: String,
    },
    Reassignment {
        id: String,
        receipt: Box<WorkItemCommandReceipt>,
    },
}
impl GrantHistoryEntry {
    /// 原始证据的稳定主键，用于续页及报告。
    pub fn id(&self) -> &str {
        match self {
            Self::Invoice { id, .. }
            | Self::Assignment { id, .. }
            | Self::InvalidAssignment { id }
            | Self::Reassignment { id, .. } => id,
        }
    }
}

/// 按来源集合的稳定主键读取下一页正式证据。
/// 最多 25 条；空页表示当前类型扫描结束。
///
/// 数据库或反序列化失败时返回错误。
///
/// db 为拥有集合的数据库，kind 为证据类型，after 为排他游标，executor 沿用调用事务。
pub async fn page(
    db: &Database,
    kind: GrantHistoryKind,
    after: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<GrantHistoryEntry>> {
    let filter = history_filter(kind, after);
    let options = FindOptions::builder().sort(doc! { "id": 1 }).limit(HISTORY_PAGE_SIZE).build();
    Ok(match kind {
        GrantHistoryKind::InvoiceApprovals => {
            mongo_ops::find_many(&db.approval_process_instances().collection(), filter, options, executor)
                .await?
                .into_iter()
                .map(|row| GrantHistoryEntry::Invoice {
                    id: row.base.id,
                    invoice_request_id: row.subject.subject_id().into(),
                })
                .collect()
        },
        GrantHistoryKind::OrderAssignments => {
            let rows =
                mongo_ops::find_many(&db.approval_node_executions().collection(), filter, options, executor)
                    .await?;
            let mut entries = Vec::with_capacity(rows.len());
            for row in rows {
                entries.push(assignment_entry(db, row, executor).await?);
            }
            entries
        },
        GrantHistoryKind::InvoiceReassignments => {
            mongo_ops::find_many(&db.work_item_command_receipts().collection(), filter, options, executor)
                .await?
                .into_iter()
                .map(|receipt| GrantHistoryEntry::Reassignment {
                    id: receipt.base.id.clone(),
                    receipt: Box::new(receipt),
                })
                .collect()
        },
    })
}

fn history_filter(kind: GrantHistoryKind, after: Option<&str>) -> Document {
    let mut filter = doc! { "deleted_at": 0 };
    if let Some(after) = after {
        filter.insert("id", doc! { "$gt": after });
    }
    match kind {
        GrantHistoryKind::InvoiceApprovals => {
            filter.insert("subject.subject_kind", "sales_invoice_request");
            filter.insert("status", "APPROVED");
        },
        GrantHistoryKind::OrderAssignments => {},
        GrantHistoryKind::InvoiceReassignments => {
            filter.insert("result.kind", "reassigned");
        },
    }
    filter
}

async fn assignment_entry(
    db: &Database,
    execution: ApprovalNodeExecution,
    executor: &mut dyn Executor,
) -> Result<GrantHistoryEntry> {
    if execution.status == ApprovalNodeExecutionStatus::Blocked {
        return Ok(GrantHistoryEntry::Assignment { id: execution.base.id, events: Vec::new() });
    }
    let instance =
        db.approval_process_instances().find_by_id(execution.process_instance_id.as_ref(), executor).await?;
    let Some(instance) = instance else {
        return Ok(GrantHistoryEntry::InvalidAssignment { id: execution.base.id });
    };
    let Ok(candidates) = assignment_events(&instance, &execution) else {
        return Ok(GrantHistoryEntry::InvalidAssignment { id: execution.base.id });
    };

    let mut events = Vec::new();
    for event in candidates {
        if assigned_to(db, &execution.base.id, &event.recipient, executor).await? {
            events.push(event);
        }
    }
    Ok(GrantHistoryEntry::Assignment { id: execution.base.id, events })
}

// 用冻结名单按账号排序、去重，历史任务核验由读取方逐条执行。
fn assignment_events(
    instance: &ApprovalProcessInstance,
    execution: &ApprovalNodeExecution,
) -> crate::Result<Vec<GrantEventEvidence>> {
    if !matches!(instance.subject.subject_kind(), "sales_order" | "purchase_order") {
        return Ok(Vec::new());
    }
    let recipients = std::iter::once(execution.assignee_participant_id.as_str())
        .chain(execution.additional_assignees.iter().map(|assignee| assignee.participant_id.as_str()))
        .collect::<BTreeSet<_>>();
    recipients.into_iter().map(|recipient| assignment_evidence(instance, execution, recipient)).collect()
}

/// 核验冻结审批人与实际生成的审批任务一一对应。
/// 已生成未删除正式任务时为 true；受阻占位执行不产生资格。
///
/// 数据库失败返回错误。
///
/// db 为 workflow 数据库，execution 为节点执行，recipient 为冻结审批人，executor 为当前事务。
async fn assigned_to(
    db: &Database,
    execution: &str,
    recipient: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    db.work_items().exists(doc! {"work_item_type":"DOCUMENT_APPROVAL", "approval_node_execution_id":execution, "responsibility_actor_ids":recipient}, executor).await
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        ApprovalParticipant, NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef,
        Timestamp,
    };
    use erp_identity::entity::resource_access::event::GrantEvent;

    use super::*;
    #[test]
    fn history_pages_bind_formal_kind_deleted_state_and_exclusive_cursor() {
        assert_eq!(
            history_filter(GrantHistoryKind::InvoiceApprovals, Some("a")),
            doc! {
                "deleted_at": 0, "id": {"$gt":"a"}, "subject.subject_kind":"sales_invoice_request", "status":"APPROVED"
            }
        );
        assert_eq!(
            history_filter(GrantHistoryKind::InvoiceReassignments, None),
            doc! {"deleted_at":0, "result.kind":"reassigned"}
        );
        assert_eq!(
            history_filter(GrantHistoryKind::OrderAssignments, Some("node")),
            doc! {"deleted_at":0, "id":{"$gt":"node"}}
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

            execution.additional_assignees.push(ApprovalParticipant {
                participant_id: ParticipantId::new("first").unwrap(),
                name: "重复参与人".into(),
            });

            let events = assignment_events(&instance, &execution).unwrap();
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
            assert!(assignment_events(&instance, &execution).unwrap().is_empty());
        }
    }
}
