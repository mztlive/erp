//! 结算草稿创建、刷新与作废命令。

use application_core::non_blank;
use erp_core::ids::SupplierAccountId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::query::SupplierSettlementStatementView;
use super::safe_command_id;

/// 草稿来源动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDraftAction {
    Create,
    Refresh,
}

/// 供应商结算单创建请求；金额明细只能由服务端来源构建器生成。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSettlementStatementRequest {
    pub action: SettlementDraftAction,
    pub supplier_id: SupplierAccountId,
    pub period_start: String,
    pub period_end: String,
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 刷新可变草稿试算请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RefreshSettlementStatementRequest {
    pub action: SettlementDraftAction,
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    #[validate(range(min = 1, message = "乐观锁版本必须大于0"))]
    pub expected_lock_version: u64,
    #[validate(length(equal = 64, message = "来源快照摘要必须为64位"))]
    pub expected_source_snapshot_hash: String,
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 草稿创建或刷新结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementDraftCommandResult {
    pub result_status: String,
    pub message: String,
    pub request_id: String,
    pub statement: SupplierSettlementStatementView,
    pub item_count: usize,
    pub difference_count: usize,
}

/// 结算单作废请求（乐观锁 + 原因）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoidSettlementRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 作废原因。
    #[validate(custom(function = "non_blank", message = "作废原因不能为空"))]
    pub reason: String,
}
