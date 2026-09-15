//! 完成动作的业务终态证据与原确认记录；工作项推进由流程承担。
use erp_core::ids::SupplierOrderActionId;
use serde::{Deserialize, Serialize};

use crate::dto::supplier_fulfillment::SupplierOrderResolution;
use crate::entity::supplier_fulfillment::*;
use crate::{Error, Result};
const COMPLETION_EVIDENCE_SCHEMA: &str = "W26_TASK_COMPLETION_V1";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionEvidenceRecord {
    schema: String,
    work_item_id: String,
    verified_evidence_id: String,
    resolution: SupplierOrderResolution,
}
impl CompletionEvidenceRecord {
    /// 原证据绑定的任务身份，只读访问。
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }
    /// 原已确认的业务终态，只读访问。
    pub fn resolution(&self) -> SupplierOrderResolution {
        self.resolution
    }
}
pub fn parse_completion_evidence(action: &SupplierOrderAction) -> Result<CompletionEvidenceRecord> {
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
/// 以当前原动作复验仍然成立的业务结果，保持关联校验先于终态比较。
pub fn ensure_current_resolution(
    order: &SupplierFulfillmentOrder,
    target_action: &SupplierOrderAction,
    resolution: SupplierOrderResolution,
) -> Result<()> {
    target_action
        .ensure_original_for_order(&order.base.id)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    if order.verified_resolution(target_action).map(Into::into) != Some(resolution) {
        return Err(Error::ConflictError("供应商结果已变化，请刷新证据后重试".to_string()));
    }
    Ok(())
}
/// 在原序列化时点构造确认动作；ID/key来自已准备的命令身份。
pub fn prepare_terminal_action(
    work_item_id: String,
    evidence: &SupplierOrderAction,
    resolution: SupplierOrderResolution,
    order_id: &str,
    terminal_action_id: String,
    completion_idempotency_key: String,
) -> Result<SupplierOrderAction> {
    let completion_record = CompletionEvidenceRecord {
        schema: COMPLETION_EVIDENCE_SCHEMA.to_string(),
        work_item_id,
        verified_evidence_id: evidence.base.id.clone(),
        resolution,
    };
    let response_summary = serde_json::to_string(&completion_record)
        .map_err(|error| Error::Internal(format!("任务完成证据序列化失败: {error}")))?;
    Ok(SupplierOrderAction::new(
        SupplierOrderActionId::new(terminal_action_id),
        SupplierOrderActionData::query_result(
            SupplierFulfillmentOrderId::new(order_id),
            completion_idempotency_key,
            format!("确认供应商结果证据 {}", evidence.base.id),
            response_summary,
        ),
    )?)
}
