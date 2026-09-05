use database::{AccessControlExt, NoTransaction, SupplierFulfillmentExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::ids::SupplierOrderActionId;
use entities::supplier_fulfillment::{
    SupplierFulfillmentOrderId, SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionStatus,
    SupplierOrderActionType,
};
use entities::work_item::WorkItemStatus;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::dto::{
    SupplierOrderResolution, SupplierOrderTaskCompletionCommand, SupplierOrderTaskCompletionResultView,
};
use super::investigate::{
    ensure_task_actor_eligible, ensure_task_subject_matches_order, validate_w26_task,
    verified_terminal_evidence,
};
use super::receipt::{
    completion_receipt_message, parse_completion_receipt, parse_positive_version, serialized_fingerprint,
    stable_digest, stable_evidence_id, stable_internal_idempotency_key, CompletionReceipt,
};
use super::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;

const COMPLETION_EVIDENCE_SCHEMA: &str = "W26_TASK_COMPLETION_V1";
const COMPLETION_AUDIT_PREFIX: &str = "w26-completion-";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompletionEvidenceRecord {
    schema: String,
    work_item_id: String,
    verified_evidence_id: String,
    resolution: SupplierOrderResolution,
}

impl SupplierFulfillmentService {
    /// 以服务端可验证终态证据完成 W26 正式任务。
    ///
    /// 命令在一个事务中重验任务/主体/订单版本、当前责任、角色资格、证据身份和
    /// 当前业务终态，追加正式确认动作后完成原任务并写入稳定审计收据。证据仍
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
        let expected_task_version = parse_positive_version(&command.expected_task_version, "任务版本")?;
        let fingerprint = serialized_fingerprint(&command)?;
        let audit_id = completion_audit_id(
            actor.id(),
            command.work_item_id.as_ref(),
            &command.idempotency_key,
        );
        if let Some(result) = self
            .replay_task_completion(&audit_id, &fingerprint, command.work_item_id.as_ref())
            .await?
        {
            return Ok(result);
        }

        let terminal_action_id = stable_evidence_id("w26c", &audit_id);
        let completion_idempotency_key = stable_internal_idempotency_key("w26c", &audit_id);
        let actor_id = actor.id().to_string();
        let actor_for_tx = actor.clone();
        let rbac_for_tx = crate::iam::shared_rbac_service(self.db.clone());
        let command_for_tx = command.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let terminal_action_id_for_tx = terminal_action_id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut work_item = db
                        .work_items()
                        .find_by_id(command_for_tx.work_item_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                    validate_w26_task(
                        &work_item,
                        command_for_tx.decision.order_id.as_ref(),
                        expected_task_version,
                        &command_for_tx.expected_subject_version,
                        &actor_id,
                    )?;
                    ensure_task_actor_eligible(&db, &work_item, &actor_id, session).await?;
                    WorkItemService::new(db.clone(), rbac_for_tx.clone())
                        .ensure_domain_decision_access(&actor_for_tx, &work_item, session)
                        .await?;

                    let order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(command_for_tx.decision.order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
                    order
                        .ensure_version(command_for_tx.decision.expected_order_lock_version)
                        .map_err(|_| {
                            Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string())
                        })?;
                    ensure_task_subject_matches_order(
                        &work_item,
                        &command_for_tx.expected_subject_version,
                        order.base.version,
                    )?;
                    let evidence = db
                        .supplier_order_actions()
                        .find_by_id(
                            command_for_tx
                                .decision
                                .verified_supplier_action_result_id
                                .as_ref(),
                            session,
                        )
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结果证据不存在".to_string()))?;
                    let evidence_record =
                        verified_terminal_evidence(&evidence, &order, command_for_tx.decision.resolution)?;
                    let target_action = db
                        .supplier_order_actions()
                        .find_by_id(&evidence_record.target_supplier_action_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("结果证据引用的原供应商动作不存在".to_string()))?;
                    target_action
                        .ensure_original_for_order(&order.base.id)
                        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                    if order.verified_resolution(&target_action).map(Into::into)
                        != Some(command_for_tx.decision.resolution)
                    {
                        return Err(Error::ConflictError(
                            "供应商结果已变化，请刷新证据后重试".to_string(),
                        ));
                    }

                    let completion_record = CompletionEvidenceRecord {
                        schema: COMPLETION_EVIDENCE_SCHEMA.to_string(),
                        work_item_id: work_item.base.id.clone(),
                        verified_evidence_id: evidence.base.id.clone(),
                        resolution: command_for_tx.decision.resolution,
                    };
                    let response_summary = serde_json::to_string(&completion_record)
                        .map_err(|error| Error::Internal(format!("任务完成证据序列化失败: {error}")))?;
                    let terminal_action = SupplierOrderAction::new(
                        SupplierOrderActionId::new(terminal_action_id_for_tx.clone()),
                        SupplierOrderActionData::query_result(
                            SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                            completion_idempotency_key,
                            format!("确认供应商结果证据 {}", evidence.base.id),
                            response_summary,
                        ),
                    )?;
                    let completed_at = Instant::now();
                    work_item.record_activity(&actor_id, completed_at)?;
                    work_item.complete_by_domain_command(&actor_id, completed_at)?;

                    db.supplier_order_actions()
                        .create(&terminal_action, session)
                        .await?;
                    db.work_items().update(&mut work_item, session).await?;
                    let receipt = CompletionReceipt {
                        terminal_action_id: terminal_action.base.id.clone(),
                        order_version: order.base.version,
                        task_version: work_item.base.version,
                        resolution: command_for_tx.decision.resolution,
                    };
                    let audit = actor_for_tx.resource_log_with_id(
                        audit_id_for_tx,
                        "supplier_fulfillment.task_complete",
                        W26_BUSINESS_OBJECT_TYPE,
                        order.base.id.clone(),
                        Some(completion_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CompletionReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;

        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(result) = self
                    .replay_task_completion(&audit_id, &fingerprint, command.work_item_id.as_ref())
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(completion_result(command.work_item_id.as_ref(), receipt))
    }

    async fn replay_task_completion(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        expected_work_item_id: &str,
    ) -> Result<Option<SupplierOrderTaskCompletionResultView>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if !audit.success
            || audit.action != "supplier_fulfillment.task_complete"
            || audit.resource_type != W26_BUSINESS_OBJECT_TYPE
        {
            return Err(Error::Internal("W26 任务完成幂等收据身份非法".to_string()));
        }
        let receipt = parse_completion_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("W26 任务完成幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        let action = self
            .db
            .supplier_order_actions()
            .find_by_id(&receipt.terminal_action_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("W26 任务完成业务证据不存在".to_string()))?;
        let record = parse_completion_evidence(&action)?;
        if record.work_item_id != expected_work_item_id || record.resolution != receipt.resolution {
            return Err(Error::Internal(
                "W26 任务完成幂等收据与业务证据不一致".to_string(),
            ));
        }
        Ok(Some(completion_result(expected_work_item_id, receipt)))
    }
}

fn parse_completion_evidence(action: &SupplierOrderAction) -> Result<CompletionEvidenceRecord> {
    if action.action_type != SupplierOrderActionType::Query
        || action.status != SupplierOrderActionStatus::Succeeded
    {
        return Err(Error::Internal("W26 任务完成证据身份非法".to_string()));
    }
    let record: CompletionEvidenceRecord = serde_json::from_str(
        action
            .response_summary
            .as_deref()
            .ok_or_else(|| Error::Internal("W26 任务完成证据为空".to_string()))?,
    )
    .map_err(|_| Error::Internal("W26 任务完成证据格式非法".to_string()))?;
    if record.schema != COMPLETION_EVIDENCE_SCHEMA {
        return Err(Error::Internal("W26 任务完成证据版本非法".to_string()));
    }
    Ok(record)
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

fn completion_audit_id(actor_id: &str, work_item_id: &str, key: &str) -> String {
    format!(
        "{COMPLETION_AUDIT_PREFIX}{}",
        stable_digest(&format!(
            "{actor_id}|supplier_fulfillment.task_complete|{work_item_id}|{key}"
        ))
    )
}
