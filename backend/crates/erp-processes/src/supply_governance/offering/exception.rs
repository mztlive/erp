//! 核对供应停止来源并完成原正式任务；不恢复供给或发布。
use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditActorLogs, AuditExt, CommandReceiptServiceExt as _};
use erp_core::common::time::Instant;
use erp_core::ids::SupplierOfferingId;
use erp_supply::dto::supplier_offering::{
    CompleteSupplierSupplyExceptionTaskRequest, CompleteSupplierSupplyExceptionTaskResult,
};
use erp_supply::repository::SupplierOfferingExt;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SupplierOfferingProcess;
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};
const SUPPLY_EXCEPTION_COMPLETE_ACTION: &str = "supplier_offering.supply_exception.complete";
impl SupplierOfferingProcess {
    /// 核对供应停止来源，并完成其唯一正式后续任务。
    ///
    /// 本命令只关闭人工核对责任，不得将“任务完成”解释为恢复供给或恢复发布。
    ///
    /// # 错误
    /// 任务、来源对象、冻结版本、当前责任或幂等请求任一不一致时失败关闭。
    pub async fn complete_supply_exception_task(
        &self,
        id: &str,
        req: CompleteSupplierSupplyExceptionTaskRequest,
        actor: &AuditActor,
    ) -> Result<CompleteSupplierSupplyExceptionTaskResult> {
        req.validate()?;
        let offering_id = id.trim();
        if offering_id.is_empty() || req.decision.offering_id.trim() != offering_id {
            return Err(Error::ValidationError("路径供给 ID 与任务决定不一致".to_string()));
        }
        let work_item_id = req.work_item_id.trim();
        let subject_version = req.expected_subject_version.trim();
        if work_item_id.is_empty() || subject_version.is_empty() {
            return Err(Error::ValidationError("任务 ID 与来源版本不能为空".to_string()));
        }
        let expected_task_version =
            erp_workflow::service::work_item::expected_task_version(&req.expected_task_version)?;
        let receipt = CommandReceipt::from_payload(
            "supplier-supply-exception:",
            actor.id(),
            SUPPLY_EXCEPTION_COMPLETE_ACTION,
            "work_item",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(committed_id) = receipt.committed_resource_id(&self.db).await? {
            return self.replay_supply_exception_completion(&committed_id, &req).await;
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let rbac = crate::adapters::identity::shared_rbac_service(db.clone());
        let actor_for_tx = actor.clone();
        let req_for_tx = req.clone();
        let receipt_for_tx = receipt.clone();
        let offering_id_for_tx = offering_id.to_string();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let typed_offering_id = SupplierOfferingId::new(&offering_id_for_tx);
                    db.supplier_offerings()
                        .find_by_id(&typed_offering_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商供给不存在".to_string()))?;
                    let mut work_item = db
                        .work_items()
                        .find_by_id(&req_for_tx.work_item_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应停止任务不存在".to_string()))?;
                    ensure_supply_exception_work_item(
                        &work_item,
                        &offering_id_for_tx,
                        expected_task_version,
                        &req_for_tx.expected_subject_version,
                    )?;
                    work_item_service(db.clone(), rbac.clone())
                        .ensure_domain_decision_access(&actor_for_tx, &work_item, executor)
                        .await?;

                    let completed_at = Instant::now();
                    work_item.complete_by_domain_command(actor_for_tx.id(), completed_at)?;
                    let decision_audit = actor_for_tx.clone().resource_log_with_message(
                        SUPPLY_EXCEPTION_COMPLETE_ACTION,
                        "supplier_offering",
                        offering_id_for_tx.clone(),
                        Some(format!(
                            "证据引用：{}；核对结论：{}",
                            req_for_tx.decision.evidence_reference.trim(),
                            req_for_tx.decision.comment.trim()
                        )),
                    )?;
                    let receipt_audit =
                        receipt_for_tx.audit(actor_for_tx.clone(), work_item.base.id.clone())?;
                    persist_completion(
                        &mut MongoCompletionWrite {
                            db: &db,
                            work_item: &mut work_item,
                            decision: &decision_audit,
                            receipt: &receipt_audit,
                        },
                        executor,
                    )
                    .await?;

                    Ok::<CompleteSupplierSupplyExceptionTaskResult, Error>(
                        supply_exception_completion_result(&work_item, &req_for_tx),
                    )
                })
            })
            .await;

        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => match receipt.committed_resource_id(&self.db).await? {
                Some(committed_id) => self.replay_supply_exception_completion(&committed_id, &req).await,
                None => Err(error),
            },
        }
    }
    async fn replay_supply_exception_completion(
        &self,
        committed_work_item_id: &str,
        req: &CompleteSupplierSupplyExceptionTaskRequest,
    ) -> Result<CompleteSupplierSupplyExceptionTaskResult> {
        if committed_work_item_id != req.work_item_id.trim() {
            return Err(Error::ConflictError("同一操作号已用于其它供应停止任务".to_string()));
        }
        let work_item = self
            .db
            .work_items()
            .find_by_id(committed_work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("已提交任务结果不存在".to_string()))?;
        if work_item.status != WorkItemStatus::Completed
            || work_item.business_object_id != req.decision.offering_id.trim()
        {
            return Err(Error::Internal("已提交供应停止任务结果不完整".to_string()));
        }
        Ok(supply_exception_completion_result(&work_item, req))
    }
}
fn ensure_supply_exception_work_item(
    work_item: &WorkItem,
    offering_id: &str,
    expected_task_version: u64,
    expected_subject_version: &str,
) -> Result<()> {
    if work_item.work_item_type != WorkItemType::BusinessException
        || work_item.business_object_type != "SUPPLIER_OFFERING"
        || work_item.business_object_id != offering_id
        || work_item.reason_code.as_deref() != Some("SUPPLIER_STOPPED")
    {
        return Err(Error::BusinessLogicError("当前任务不是已注册的供应停止核对任务".to_string()));
    }
    if work_item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("供应停止任务已不再开放".to_string()));
    }
    if work_item.base.version != expected_task_version {
        return Err(Error::ConflictError("任务已被其他请求修改，请刷新后重试".to_string()));
    }
    if work_item.subject_version != expected_subject_version.trim() {
        return Err(Error::ConflictError("供应停止来源版本已变化，请刷新后重试".to_string()));
    }
    Ok(())
}
fn supply_exception_completion_result(
    work_item: &WorkItem,
    req: &CompleteSupplierSupplyExceptionTaskRequest,
) -> CompleteSupplierSupplyExceptionTaskResult {
    CompleteSupplierSupplyExceptionTaskResult {
        work_item_id: req.work_item_id.trim().to_string(),
        safety_pause_operation_id: work_item.base.id.clone(),
        evidence_reference: req.decision.evidence_reference.trim().to_string(),
        message: "供应停止来源已核对；任务已完成".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};

    use super::*;
    #[test]
    fn supply_exception_completion_requires_exact_frozen_task_identity() {
        let task = supply_exception_task();

        ensure_supply_exception_work_item(&task, "offering-1", task.base.version, "offering:2").unwrap();
        assert!(
            ensure_supply_exception_work_item(&task, "offering-2", task.base.version, "offering:2",).is_err()
        );
        assert!(
            ensure_supply_exception_work_item(&task, "offering-1", task.base.version, "offering:3",).is_err()
        );
    }
    fn supply_exception_task() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("work-item-1"),
            WorkItemData {
                work_item_type: WorkItemType::BusinessException,
                business_object_type: "SUPPLIER_OFFERING".to_string(),
                business_object_id: "offering-1".to_string(),
                subject_version: "offering:2".to_string(),
                owner_role: "role-operations".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "operator-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("SUPPLIER_STOPPED".to_string()),
                impact_summary: Some("供应停止待核对".to_string()),
            },
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
trait CompletionWritePort: Send {
    async fn task(&mut self, executor: &mut dyn persistence_core::Executor) -> Result<()>;
    async fn decision(&mut self, executor: &mut dyn persistence_core::Executor) -> Result<()>;
    async fn receipt(&mut self, executor: &mut dyn persistence_core::Executor) -> Result<()>;
}
struct MongoCompletionWrite<'a> {
    db: &'a mongodb::Database,
    work_item: &'a mut WorkItem,
    decision: &'a erp_audit::AuditLog,
    receipt: &'a erp_audit::AuditLog,
}
#[async_trait::async_trait]
impl CompletionWritePort for MongoCompletionWrite<'_> {
    async fn task(&mut self, executor: &mut dyn persistence_core::Executor) -> Result<()> {
        self.db.work_items().update(self.work_item, executor).await.map_err(Into::into)
    }
    async fn decision(&mut self, executor: &mut dyn persistence_core::Executor) -> Result<()> {
        self.db.audit_logs().create(self.decision, executor).await.map_err(Into::into)
    }
    async fn receipt(&mut self, executor: &mut dyn persistence_core::Executor) -> Result<()> {
        self.db.audit_logs().create(self.receipt, executor).await.map_err(Into::into)
    }
}
/// 原任务CAS、决定审计、收据审计顺序；任一失败停止后续步骤。
async fn persist_completion<P: CompletionWritePort>(
    port: &mut P,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    port.task(executor).await?;
    port.decision(executor).await?;
    port.receipt(executor).await
}
#[cfg(test)]
mod completion_tests {
    use persistence_core::Executor;

    use super::*;
    struct Marker(u64);
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        pointer: usize,
        calls: Vec<&'static str>,
        fail: Option<usize>,
    }
    impl Recorder {
        fn record(&mut self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            let i = self.calls.len();
            self.calls.push(name);
            if self.fail == Some(i) {
                return Err(Error::ConflictError(format!("completion {i}")));
            }
            Ok(())
        }
    }
    #[async_trait::async_trait]
    impl CompletionWritePort for Recorder {
        async fn task(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("task", executor)
        }
        async fn decision(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("decision", executor)
        }
        async fn receipt(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("receipt", executor)
        }
    }
    #[tokio::test]
    async fn completion_preserves_executor_and_stops_each_original_write_failure() {
        for fail in [None, Some(0), Some(1), Some(2)] {
            let mut executor = Marker(165);
            let mut port = Recorder { pointer: &mut executor as *mut Marker as usize, calls: vec![], fail };
            let result = persist_completion(&mut port, &mut executor).await;
            if let Some(i) = fail {
                assert!(matches!(result,Err(Error::ConflictError(ref e)) if e==&format!("completion {i}")));
                assert_eq!(port.calls, ["task", "decision", "receipt"][..=i]);
            } else {
                result.unwrap();
                assert_eq!(port.calls, ["task", "decision", "receipt"]);
            }
            assert_eq!(executor.0, 165);
        }
    }
}
