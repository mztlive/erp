//! 结算差异补证与正式决定命令。

use application_core::non_blank;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::query::{SettlementDifferenceEvidenceView, SupplierSettlementDifferenceView};
use super::safe_command_id;

/// 差异补证强命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementDifferenceEvidenceRequest {
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    #[validate(length(min = 1, max = 128, message = "差异ID长度必须在1-128之间"))]
    pub difference_id: String,
    #[validate(range(min = 1, message = "差异版本必须大于0"))]
    pub expected_difference_version: u64,
    #[validate(length(min = 1, max = 20, message = "证据引用必须在1-20项之间"))]
    pub evidence_reference_ids: Vec<String>,
    pub opinion_code: Option<String>,
    #[validate(length(max = 1024, message = "补证说明不能超过1024字"))]
    pub comment: Option<String>,
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 差异补证命令结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementDifferenceEvidenceResult {
    pub result_status: String,
    pub message: String,
    pub request_id: String,
    pub statement_id: String,
    pub difference_id: String,
    pub evidence: SettlementDifferenceEvidenceView,
}

/// 结算差异正式处理结论。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceResolution {
    /// 供应商接受 ERP 口径。
    SupplierAccepted,
    /// ERP 接受供应商口径。
    ErpAccepted,
    /// 已通过独立补偿事实处理。
    Compensated,
    /// 有证据证明无需金额调整并关闭。
    ClosedNoAdjustment,
}

/// 结算差异强类型决定请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementDifferenceDecisionRequest {
    /// 所属结算单；必须与差异归属一致。
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    /// 差异 ID；必须与路径一致。
    #[validate(length(min = 1, max = 128, message = "差异ID长度必须在1-128之间"))]
    pub difference_id: String,
    /// 查询所得结算单版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 查询所得差异版本。
    #[validate(range(min = 1, message = "差异版本必须大于 0"))]
    pub expected_difference_version: u64,
    /// 固定正式结论。
    pub resolution: SettlementDifferenceResolution,
    /// 受控原因代码。
    #[validate(custom(function = "non_blank", message = "原因代码不能为空"))]
    pub reason_code: String,
    /// 正式证据引用；补偿或无调整关闭至少一项。
    pub evidence_reference_ids: Vec<String>,
    /// 客户端稳定操作 ID。
    #[validate(length(min = 1, max = 64, message = "操作ID长度必须在1-64之间"))]
    #[validate(custom(function = "safe_command_id", message = "操作ID格式非法"))]
    pub operation_id: String,
    /// 正式命令幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 差异决定结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceDecisionStatus {
    /// 差异正式结论已登记。
    Resolved,
}

/// 结算差异决定结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementDifferenceDecisionResult {
    /// 固定结果状态。
    pub result_status: SettlementDifferenceDecisionStatus,
    /// 面向用户的稳定说明。
    pub message: String,
    /// 原请求操作 ID。
    pub operation_id: String,
    /// 所属结算单。
    pub statement_id: String,
    /// 差异决定后推进的结算单版本。
    pub statement_lock_version: u64,
    /// 正式差异投影。
    pub difference: SupplierSettlementDifferenceView,
}
