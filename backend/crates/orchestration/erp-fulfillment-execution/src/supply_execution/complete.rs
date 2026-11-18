use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::internal;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{SupplyCommandReceipt, SupplyCommandResult};
use erp_supply::dto::supplier_fulfillment::SupplierOrderTaskCompletionCommand;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_fulfillment::complete::{
    ensure_current_resolution, parse_completion_evidence, prepare_terminal_action,
};
use erp_supply::service::supplier_fulfillment::investigate::{create_action, verified_terminal_evidence};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::dto::SupplierOrderTaskCompletionResultView;
use super::investigate::{ensure_task_subject_matches_order, validate_w26_task};
use super::receipt::{
    CompletionReceipt, parse_positive_version, persist_supply_receipt, serialized_fingerprint, stable_digest,
    stable_evidence_id, stable_internal_idempotency_key,
};
use super::{
    SupplierFulfillmentIdentity, SupplierFulfillmentProcess, W26_BUSINESS_OBJECT_TYPE, legacy,
    task_authorization,
};
use crate::{Error, Result};

const COMPLETION_COMMAND_PREFIX: &str = "w26-completion-";

impl SupplierFulfillmentProcess {
    /// 以服务端可验证终态证据完成 W26 正式任务。
    ///
    /// 命令在一个事务中重验任务/主体/订单版本、当前责任、角色资格、证据身份和
    /// 当前业务终态，追加正式确认动作后完成原任务并写入独立命令回执。证据仍
    /// 未知或任务误派时保持原任务开放。
    ///
    /// # 参数
    /// * `command` - W26 任务完成命令。
    /// * `actor` - 当前任务责任人。
    ///
    /// # 返回
    /// 返回任务完成结果；同一命令重放返回原结果。证据仍未知或任务误派时保持原任务开放。
    ///
    /// # 错误
    /// 任一任务、责任、版本、证据或终态不变量不成立，以及幂等键复用不同命令时
    /// 失败关闭。
    pub async fn complete_order_task(
        &self,
        command: SupplierOrderTaskCompletionCommand,
        actor: &AuditActor,
    ) -> Result<SupplierOrderTaskCompletionResultView> {
        command.validate()?;
        let expected_task_version = parse_positive_version(&command.expected_task_version, "任务版本")?;
        let fingerprint = serialized_fingerprint(&command)?;
        let command_id =
            completion_command_id(actor.id(), command.work_item_id.as_ref(), &command.idempotency_key);

        if let Some(result) = self
            .replay_task_completion(
                &command_id,
                &fingerprint,
                command.work_item_id.as_ref(),
                actor,
                &command.idempotency_key,
            )
            .await?
        {
            return Ok(result);
        }

        let completion = TaskCompletion::new(
            self.identity.clone(),
            &command,
            actor,
            expected_task_version,
            &command_id,
            &fingerprint,
        );
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |executor| Box::pin(async move { completion.apply(&db, executor).await }))
            .await;

        super::execution::recover_final_result(
            transaction_result.map(|receipt| completion_result(command.work_item_id.as_ref(), receipt)),
            || {
                self.replay_task_completion(
                    &command_id,
                    &fingerprint,
                    command.work_item_id.as_ref(),
                    actor,
                    &command.idempotency_key,
                )
            },
        )
        .await
    }

    /// 沿当前账号完整执行资格回读精确成功回执，不要求已完成任务仍开放。
    async fn replay_task_completion(
        &self,
        command_id: &str,
        expected_fingerprint: &str,
        expected_work_item_id: &str,
        actor: &AuditActor,
        idempotency_key: &str,
    ) -> Result<Option<SupplierOrderTaskCompletionResultView>> {
        let db = self.db.clone();
        let authorization = self.identity.rbac.clone();
        let replay = CompletionReplay {
            command_id: command_id.into(),
            fingerprint: expected_fingerprint.into(),
            work_item_id: expected_work_item_id.into(),
            actor: actor.clone(),
            idempotency_key: idempotency_key.into(),
        };
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { replay.load(&db, &authorization, executor).await })
            })
            .await
    }
}

/// 冻结原回放请求身份；授权服务按当前账号取证，不保存旧授权结果。
struct CompletionReplay {
    command_id: String,
    fingerprint: String,
    work_item_id: String,
    actor: AuditActor,
    idempotency_key: String,
}

impl CompletionReplay {
    /// 当前完整功能资格、原回执和业务终态证据均沿调用方同一快照读取。
    async fn load(
        &self,
        db: &Database,
        authorization: &AuthorizationService,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderTaskCompletionResultView>> {
        authorization
            .require(
                &self.actor,
                &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        let Some(stored) = db.supply_command_receipts().find_command(&self.command_id, executor).await?
        else {
            return Ok(None);
        };
        let (resource_id, receipt) = self.receipt(stored)?;
        legacy::ensure(db, &resource_id, executor).await?;

        let action = db
            .supplier_order_actions()
            .find_by_id(&receipt.terminal_action_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("W26 任务完成业务证据不存在".to_string()))?;
        let record = parse_completion_evidence(&action)?;
        if action.supplier_fulfillment_order_id.as_ref() != resource_id
            || record.work_item_id() != self.work_item_id
            || record.resolution() != receipt.resolution
        {
            return Err(Error::Internal("W26 任务完成幂等收据与业务证据不一致".to_string()));
        }
        Ok(Some(completion_result(&self.work_item_id, receipt)))
    }

    /// 只接收原操作人、任务、幂等键及载荷完全匹配的终态回执。
    fn receipt(&self, stored: SupplyCommandReceipt) -> Result<(String, CompletionReceipt)> {
        stored.verify_identity(
            &self.command_id,
            self.actor.id(),
            "supplier_fulfillment.task_complete",
            &self.work_item_id,
            &stable_digest(self.idempotency_key.trim()),
        )?;
        stored.verify(&self.fingerprint, None, "请求标识已用于不同的任务完成命令")?;
        let resource_id = stored.resource_id;
        let SupplyCommandResult::Completion(receipt) = stored.result else {
            return Err(Error::Internal("W26 任务完成幂等收据身份非法".to_string()));
        };
        Ok((resource_id, receipt))
    }
}

struct TaskCompletion {
    terminal_action_id: String,
    idempotency_key: String,
    actor: AuditActor,
    identity: SupplierFulfillmentIdentity,
    command: SupplierOrderTaskCompletionCommand,
    expected_task_version: u64,
    command_id: String,
    fingerprint: String,
}

impl TaskCompletion {
    fn new(
        identity: SupplierFulfillmentIdentity,
        command: &SupplierOrderTaskCompletionCommand,
        actor: &AuditActor,
        expected_task_version: u64,
        command_id: &str,
        fingerprint: &str,
    ) -> Self {
        Self {
            terminal_action_id: stable_evidence_id("w26c", command_id),
            idempotency_key: stable_internal_idempotency_key("w26c", command_id),
            actor: actor.clone(),
            identity,
            command: command.clone(),
            expected_task_version,
            command_id: command_id.to_string(),
            fingerprint: fingerprint.to_string(),
        }
    }

    async fn apply(self, db: &Database, executor: &mut dyn Executor) -> Result<CompletionReceipt> {
        let mut work_item = load_completion_task(
            db,
            &self.command,
            &self.actor,
            &self.identity,
            self.expected_task_version,
            executor,
        )
        .await?;
        legacy::ensure(db, self.command.decision.order_id.as_ref(), executor).await?;

        let (order, evidence) = load_completion_evidence(db, &self.command, &work_item, executor).await?;
        let terminal_action = prepare_terminal_action(
            work_item.base.id.clone(),
            &evidence,
            self.command.decision.resolution,
            order.base.id.as_str(),
            self.terminal_action_id,
            self.idempotency_key,
        )?;

        let completed_at = Instant::now();
        work_item.record_activity(completed_at)?;
        work_item.complete_by_domain_command(self.actor.id(), completed_at)?;
        persist_completed_task(
            CompletionWrite {
                db,
                work_item: &mut work_item,
                order: &order,
                terminal_action: &terminal_action,
                command: &self.command,
                actor: &self.actor,
                command_id: &self.command_id,
                fingerprint: &self.fingerprint,
            },
            executor,
        )
        .await
    }
}

async fn load_completion_evidence(
    db: &Database,
    command: &SupplierOrderTaskCompletionCommand,
    work_item: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<(SupplierFulfillmentOrder, SupplierOrderAction)> {
    let order = db
        .supplier_fulfillment_orders()
        .find_by_id(command.decision.order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
    order
        .ensure_version(command.decision.expected_order_lock_version)
        .map_err(|_| Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string()))?;
    ensure_task_subject_matches_order(work_item, &command.expected_subject_version, order.base.version)?;

    let evidence = db
        .supplier_order_actions()
        .find_by_id(command.decision.verified_supplier_action_result_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商结果证据不存在".to_string()))?;
    let evidence_record = verified_terminal_evidence(&evidence, &order, command.decision.resolution)?;

    let target_action = db
        .supplier_order_actions()
        .find_by_id(evidence_record.target_supplier_action_id(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("结果证据引用的原供应商动作不存在".to_string()))?;
    ensure_current_resolution(&order, &target_action, command.decision.resolution)?;
    Ok((order, evidence))
}

fn completion_result(
    work_item_id: &str,
    receipt: CompletionReceipt,
) -> SupplierOrderTaskCompletionResultView {
    SupplierOrderTaskCompletionResultView {
        operation_id: receipt.terminal_action_id,
        work_item_id: work_item_id.to_string(),
        work_item_status: WorkItemStatus::Completed,
        task_version: receipt.task_version,
        order_lock_version: receipt.order_version,
        resolution: receipt.resolution,
    }
}

fn completion_command_id(actor_id: &str, work_item_id: &str, key: &str) -> String {
    format!(
        "{COMPLETION_COMMAND_PREFIX}{}",
        stable_digest(&format!("{actor_id}|supplier_fulfillment.task_complete|{work_item_id}|{key}"))
    )
}

/// 保持正式确认动作、任务、独立回执与事件的原写入顺序。
struct CompletionWrite<'a> {
    db: &'a Database,
    work_item: &'a mut WorkItem,
    order: &'a SupplierFulfillmentOrder,
    terminal_action: &'a SupplierOrderAction,
    command: &'a SupplierOrderTaskCompletionCommand,
    actor: &'a AuditActor,
    command_id: &'a str,
    fingerprint: &'a str,
}

/// 使用调用方的执行器写入完成结果，不改变失败停止边界。
async fn persist_completed_task(
    input: CompletionWrite<'_>,
    executor: &mut dyn Executor,
) -> Result<CompletionReceipt> {
    create_action(input.db, input.terminal_action, executor).await?;
    input.db.work_items().update(input.work_item, executor).await?;

    let receipt = CompletionReceipt {
        terminal_action_id: input.terminal_action.base.id.clone(),
        order_version: input.order.base.version,
        task_version: input.work_item.base.version,
        resolution: input.command.decision.resolution,
    };
    let audit = input
        .actor
        .clone()
        .resource_log_with_id(
            input.command_id.to_string(),
            "supplier_fulfillment.task_complete",
            W26_BUSINESS_OBJECT_TYPE,
            input.order.base.id.clone(),
            Some("供应商履约任务已完成".to_string()),
        )?
        .with_command_id(Some(input.command_id.to_string()))?
        .with_resource_number(Some(input.order.fulfillment_order_no.clone()))?;
    persist_supply_receipt(
        input.db,
        &audit,
        input.fingerprint,
        &input.command.idempotency_key,
        input.command.work_item_id.as_ref(),
        SupplyCommandResult::Completion(receipt.clone()),
        executor,
    )
    .await?;
    persist_log(input.db, &audit, executor).await?;
    Ok(receipt)
}

/// 先以精确任务目标授权，再核对正式任务、冻结版本及业务责任。
async fn load_completion_task(
    db: &Database,
    command: &SupplierOrderTaskCompletionCommand,
    actor: &AuditActor,
    identity: &SupplierFulfillmentIdentity,
    expected_task_version: u64,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    task_authorization::require_target(
        &identity.rbac,
        actor,
        command.work_item_id.as_ref(),
        expected_task_version,
        executor,
    )
    .await?;

    let work_item = db
        .work_items()
        .find_by_id(command.work_item_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
    validate_w26_task(
        &work_item,
        command.decision.order_id.as_ref(),
        expected_task_version,
        &command.expected_subject_version,
    )?;
    identity.workflow.ensure_domain_decision_access(actor, &work_item, executor).await?;
    Ok(work_item)
}

#[cfg(test)]
mod replay_tests {
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_supply::dto::supplier_fulfillment::SupplierOrderResolution;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 同一成功命令的冻结请求和独立回执，不依赖当前任务状态。
    fn fixture() -> (CompletionReplay, SupplyCommandReceipt) {
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let mut base = BaseModel::fake();
        base.id = "completion-command".into();
        let fingerprint = stable_digest("original request");
        let replay = CompletionReplay {
            command_id: base.id.clone(),
            fingerprint: fingerprint.clone(),
            work_item_id: "completed-task".into(),
            actor,
            idempotency_key: "original-key".into(),
        };
        let stored = SupplyCommandReceipt {
            base,
            schema_version: 1,
            actor_id: "actor".into(),
            action: "supplier_fulfillment.task_complete".into(),
            resource_type: W26_BUSINESS_OBJECT_TYPE.into(),
            resource_id: "order".into(),
            scope_id: "completed-task".into(),
            idempotency_key_hash: stable_digest("original-key"),
            fingerprint,
            fingerprint_algorithm: "sha256-serialized-v1".into(),
            result: SupplyCommandResult::Completion(CompletionReceipt {
                terminal_action_id: "terminal-evidence".into(),
                order_version: 5,
                task_version: 8,
                resolution: SupplierOrderResolution::OrderCompleted,
            }),
            audit_event_id: "audit".into(),
        };
        (replay, stored)
    }

    #[tokio::test]
    async fn current_authorization_failure_stops_before_receipts_or_tasks_are_read() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let (replay, _) = fixture();
        let authorization = unavailable_authorization();

        let error = replay.load(&db, &authorization, &mut SnapshotExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));

        let error = replay.load(&db, &authorization, &mut ReadExecutor::default()).await.unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "无对象操作及其业务读取必须使用快照事务")
        );
    }

    #[test]
    fn matching_completed_receipt_preserves_original_result_and_versions() {
        let (replay, stored) = fixture();
        let (order, receipt) = replay.receipt(stored).unwrap();
        let result = completion_result(&replay.work_item_id, receipt);
        assert_eq!(order, "order");
        assert_eq!(result.work_item_id, "completed-task");
        assert_eq!(result.operation_id, "terminal-evidence");
        assert_eq!(result.work_item_status, WorkItemStatus::Completed);
        assert_eq!(result.task_version, 8);
        assert_eq!(result.order_lock_version, 5);
        assert_eq!(result.resolution, SupplierOrderResolution::OrderCompleted);
    }

    #[test]
    fn another_actor_task_or_key_cannot_replay_the_original_receipt() {
        let mutations: [fn(&mut CompletionReplay); 3] = [
            |replay| replay.actor = AuditActor::new("other".into(), "other".into(), AccountKind::Admin),
            |replay| replay.work_item_id = "other-task".into(),
            |replay| replay.idempotency_key = "other-key".into(),
        ];
        for mutate in mutations {
            let (mut replay, stored) = fixture();
            mutate(&mut replay);
            assert!(matches!(replay.receipt(stored), Err(Error::Internal(_))));
        }

        let (mut replay, stored) = fixture();
        replay.fingerprint = stable_digest("changed request");
        assert!(matches!(replay.receipt(stored), Err(Error::ConflictError(_))));
    }
}
