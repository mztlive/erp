use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_supply::command_receipt::SupplyCommandResult;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::dto::supplier_fulfillment::SupplierOrderTaskCompletionCommand;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::investigate::{ensure_task_subject_matches_order, validate_w26_task};
use super::receipt::{
    CompletionReceipt, parse_positive_version, persist_supply_receipt, serialized_fingerprint, stable_digest,
    stable_evidence_id, stable_internal_idempotency_key,
};
use super::{SupplierFulfillmentProcess, W26_BUSINESS_OBJECT_TYPE};
use crate::adapters::workflow::work_item_service;
use crate::audit::persist_log;
use crate::{Error, Result};

const COMPLETION_COMMAND_PREFIX: &str = "w26-completion-";

impl SupplierFulfillmentProcess {
    /// 以服务端可验证终态证据完成 W26 正式任务。
    ///
    /// 命令在一个事务中重验任务/主体/订单版本、当前责任、角色资格、证据身份和
    /// 当前业务终态，追加正式确认动作后完成原任务并写入独立命令回执。证据仍
    /// 未知或任务误派时保持原任务开放。
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
        self.require_task_permissions(actor, &mut NoTransaction).await?;
        let expected_task_version = parse_positive_version(&command.expected_task_version, "任务版本")?;
        let fingerprint = serialized_fingerprint(&command)?;
        let command_id =
            completion_command_id(actor.id(), command.work_item_id.as_ref(), &command.idempotency_key);
        if let Some(result) = self
            .replay_task_completion(
                &command_id,
                &fingerprint,
                command.work_item_id.as_ref(),
                (actor.id(), &command.idempotency_key),
            )
            .await?
        {
            return Ok(result);
        }

        let terminal_action_id = stable_evidence_id("w26c", &command_id);
        let completion_idempotency_key = stable_internal_idempotency_key("w26c", &command_id);
        let actor_id = actor.id().to_string();
        let actor_for_tx = actor.clone();
        let rbac_for_tx = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let command_for_tx = command.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let command_id_for_tx = command_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let terminal_action_id_for_tx = terminal_action_id.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut work_item = load_completion_task(
                        &db,
                        &command_for_tx,
                        &actor_for_tx,
                        &rbac_for_tx,
                        expected_task_version,
                        executor,
                    )
                    .await?;

                    let order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(command_for_tx.decision.order_id.as_ref(), executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
                    order.ensure_version(command_for_tx.decision.expected_order_lock_version).map_err(
                        |_| Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string()),
                    )?;
                    ensure_task_subject_matches_order(
                        &work_item,
                        &command_for_tx.expected_subject_version,
                        order.base.version,
                    )?;
                    let evidence = db
                        .supplier_order_actions()
                        .find_by_id(
                            command_for_tx.decision.verified_supplier_action_result_id.as_ref(),
                            executor,
                        )
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结果证据不存在".to_string()))?;
                    let evidence_record =
                        verified_terminal_evidence(&evidence, &order, command_for_tx.decision.resolution)?;
                    let target_action = db
                        .supplier_order_actions()
                        .find_by_id(evidence_record.target_supplier_action_id(), executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("结果证据引用的原供应商动作不存在".to_string()))?;
                    ensure_current_resolution(&order, &target_action, command_for_tx.decision.resolution)?;
                    let terminal_action = prepare_terminal_action(
                        work_item.base.id.clone(),
                        &evidence,
                        command_for_tx.decision.resolution,
                        order.base.id.as_str(),
                        terminal_action_id_for_tx.clone(),
                        completion_idempotency_key,
                    )?;
                    let completed_at = Instant::now();
                    work_item.record_activity(&actor_id, completed_at)?;
                    work_item.complete_by_domain_command(&actor_id, completed_at)?;

                    persist_completed_task(
                        CompletionWrite {
                            db: &db,
                            work_item: &mut work_item,
                            order: &order,
                            terminal_action: &terminal_action,
                            command: &command_for_tx,
                            actor: &actor_for_tx,
                            command_id: &command_id_for_tx,
                            fingerprint: &fingerprint_for_tx,
                        },
                        executor,
                    )
                    .await
                })
            })
            .await;

        super::execution::recover_final_result(
            transaction_result.map(|receipt| completion_result(command.work_item_id.as_ref(), receipt)),
            || {
                self.replay_task_completion(
                    &command_id,
                    &fingerprint,
                    command.work_item_id.as_ref(),
                    (actor.id(), &command.idempotency_key),
                )
            },
        )
        .await
    }

    async fn replay_task_completion(
        &self,
        command_id: &str,
        expected_fingerprint: &str,
        expected_work_item_id: &str,
        identity: (&str, &str),
    ) -> Result<Option<SupplierOrderTaskCompletionResultView>> {
        let Some(stored) =
            self.db.supply_command_receipts().find_command(command_id, &mut NoTransaction).await?
        else {
            return Ok(None);
        };
        stored.verify_identity(
            command_id,
            identity.0,
            "supplier_fulfillment.task_complete",
            expected_work_item_id,
            &stable_digest(identity.1.trim()),
        )?;
        stored.verify(expected_fingerprint, None, "请求标识已用于不同的任务完成命令")?;
        let resource_id = stored.resource_id;
        let SupplyCommandResult::Completion(receipt) = stored.result else {
            return Err(Error::Internal("W26 任务完成幂等收据身份非法".to_string()));
        };
        let action = self
            .db
            .supplier_order_actions()
            .find_by_id(&receipt.terminal_action_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("W26 任务完成业务证据不存在".to_string()))?;
        let record = parse_completion_evidence(&action)?;
        if action.supplier_fulfillment_order_id.as_ref() != resource_id
            || record.work_item_id() != expected_work_item_id
            || record.resolution() != receipt.resolution
        {
            return Err(Error::Internal("W26 任务完成幂等收据与业务证据不一致".to_string()));
        }
        Ok(Some(completion_result(expected_work_item_id, receipt)))
    }
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

use erp_read_models::supplier_center::fulfillment_access::ensure_task_actor_eligible;
use erp_supply::service::supplier_fulfillment::complete::{
    ensure_current_resolution, parse_completion_evidence, prepare_terminal_action,
};
use erp_supply::service::supplier_fulfillment::investigate::{create_action, verified_terminal_evidence};

use super::dto::SupplierOrderTaskCompletionResultView;

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

/// 沿原次序重验正式任务、个人责任与当前授权。
async fn load_completion_task(
    db: &Database,
    command: &SupplierOrderTaskCompletionCommand,
    actor: &AuditActor,
    rbac: &SharedRbacService,
    expected_task_version: u64,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
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
        actor.id(),
    )?;
    ensure_task_actor_eligible(db, &work_item, actor.id(), executor).await?;
    work_item_service(db.clone(), rbac.clone())
        .ensure_domain_decision_access(actor, &work_item, executor)
        .await?;

    Ok(work_item)
}
