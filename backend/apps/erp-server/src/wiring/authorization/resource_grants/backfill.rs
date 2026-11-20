//! 存量证据回填只追加授权事件，不重放审批、转派或财务命令。
use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_finance::repository::extensions::ReceivableExt;
use erp_identity::dto::validate_grant_cursor;
use erp_identity::entity::resource_access::event::{GrantEvent, GrantEventEvidence, GrantEventPayload};
use erp_identity::ports::ResourceGrantFacts;
use erp_identity::service::resource_access::append_event;
use erp_identity::{Error, Result};
use erp_workflow::entity::work_item_command::WorkItemCommandResult;
use erp_workflow::repository::WorkItemExt;
pub use erp_workflow::repository::grant_history::GrantHistoryKind;
use erp_workflow::repository::grant_history::{self, GrantHistoryEntry};
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use super::ResourceGrantRuntime;
use super::facts::Facts;

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

impl ResourceGrantRuntime {
    /// 校验管理员权限后，按指定证据类型和排他游标处理一页历史记录。预览只统计，执行只追加授权事件，不重放已有审批、转派或财务命令。
    /// 报告区分新增、已有、永久终止和无效证据，空页代表本类扫描结束。权限或基础设施错误中止整页，提交结果未知时不自动重试。
    pub async fn backfill(&self, actor: &AuditActor, request: BackfillRequest) -> Result<BackfillReport> {
        validate_grant_cursor(request.after.as_deref())?;

        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |ex| {
                Box::pin(async move {
                    this.grants.authorize_manager(&actor, ex).await?;
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
            let id = entry.id().to_owned();
            let prepared = match self.prepare_history(entry, &planned, ex).await {
                Ok(value) => value,
                Err(Error::ConflictError(reason)) => {
                    report.invalid.push(BackfillProblem { record_id: id, reason });
                    continue;
                },
                Err(Error::ValidationError(_)) => {
                    report.invalid.push(BackfillProblem {
                        record_id: id,
                        reason: "原审批或人员分派资料不符合补齐查看权限的要求，请核对该笔记录".into(),
                    });
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
        entry: GrantHistoryEntry,
        planned: &BTreeSet<String>,
        ex: &mut dyn Executor,
    ) -> Result<PreparedHistory> {
        let events = self.historical_events(entry, ex).await?;
        let mut prepared = PreparedHistory::default();
        let facts = Facts { db: self.db.clone() };
        for evidence in events {
            let event = GrantEvent::new(evidence)?;
            if self.queue.source(&event.source, ex).await?.is_some_and(|source| source.terminated) {
                prepared.terminated += 1;
                continue;
            }

            let update = facts.compute_source(&event, ex).await?;
            facts.validate_targets(&update.targets, ex).await?;

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
        entry: GrantHistoryEntry,
        ex: &mut dyn Executor,
    ) -> Result<Vec<GrantEventEvidence>> {
        match entry {
            GrantHistoryEntry::Invoice { id, invoice_request_id } => {
                let request = self
                    .db
                    .sales_invoice_requests()
                    .find_by_id(&invoice_request_id, ex)
                    .await?
                    .ok_or_else(invalid)?;
                let task_id = request.work_item_id.as_deref().ok_or_else(invalid)?;
                let task = self.db.work_items().find_by_id(task_id, ex).await?.ok_or_else(invalid)?;
                let recipient = task.responsibility_actor_ids.first().ok_or_else(invalid)?.clone();

                Ok(vec![GrantEventEvidence {
                    payload: GrantEventPayload::InvoiceApproved {
                        invoice_request_id: request.base.id.clone(),
                        approval_instance_id: id,
                        work_item_id: task.base.id,
                    },
                    recipient,
                    fact_version: request.base.version,
                    occurred_at: request.base.updated_at,
                }])
            },
            GrantHistoryEntry::Assignment { events, .. } => Ok(events),
            GrantHistoryEntry::InvalidAssignment { .. } => Err(invalid()),
            GrantHistoryEntry::Reassignment { receipt, .. } => {
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
    Error::ConflictError("原审批、待办或人员分派记录缺失或不一致，无法补齐查看权限，请核对该笔记录".into())
}

#[cfg(test)]
mod tests {
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
}
