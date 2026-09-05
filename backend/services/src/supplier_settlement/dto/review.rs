//! 结算复核提交与决定命令。

use entities::money::Amount;
use entities::supplier_settlement::SettlementReviewRejectReason;
use entities::work_item::WorkItemStatus;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::query::SupplierSettlementStatementView;
use super::safe_command_id;
use crate::errors::Result;
use crate::query::non_blank;

/// 结算单对象级提交动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementObjectAction {
    /// 提交财务复核。
    SubmitReview,
}

/// 提交结算复核强类型请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitSettlementReviewRequest {
    /// 固定对象级动作。
    pub action: SettlementObjectAction,
    /// 结算单 ID；必须与路径一致。
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    /// 查询所得结算单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 查询所得服务端主题摘要。
    #[validate(length(equal = 64, message = "主题摘要必须为64位"))]
    pub subject_hash: String,
    /// 查询所得刷新截止策略 ID。
    #[validate(custom(function = "non_blank", message = "刷新截止策略不能为空"))]
    pub refresh_cutoff_policy_id: String,
    /// 查询所得刷新截止策略版本。
    #[validate(custom(function = "non_blank", message = "刷新截止策略版本不能为空"))]
    pub expected_refresh_cutoff_policy_version: String,
    /// 本次复核任务的明确责任人。
    #[validate(
        custom(function = "non_blank", message = "复核人不能为空"),
        length(max = 128, message = "复核人ID不能超过128个字符")
    )]
    pub reviewer_user_id: String,
    /// 客户端稳定操作 ID，用于结果查询关联。
    #[validate(length(min = 1, max = 64, message = "操作ID长度必须在1-64之间"))]
    #[validate(custom(function = "safe_command_id", message = "操作ID格式非法"))]
    pub operation_id: String,
    /// 正式命令幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 提交说明。
    #[validate(length(max = 512, message = "提交说明长度不能超过512"))]
    pub comment: Option<String>,
}

/// 提交复核结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewSubmissionStatus {
    /// 结算主题与唯一复核任务已原子提交。
    Submitted,
}

/// 提交结算复核结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitSettlementReviewResult {
    /// 固定结果状态。
    pub result_status: SettlementReviewSubmissionStatus,
    /// 面向用户的稳定结果说明。
    pub message: String,
    /// 原请求操作 ID。
    pub operation_id: String,
    /// 提交后的结算单投影。
    pub statement: SupplierSettlementStatementView,
    /// 同事务创建的正式复核任务。
    pub work_item_id: String,
}

/// 供应商结算复核动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewAction {
    /// 驳回给经办人。
    Reject,
    /// 确认并形成应付与成本差额。
    Confirm,
}

/// 供应商结算复核决定载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementReviewDecisionData {
    /// 结算单 ID；必须与路径一致。
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    /// 查询所得结算单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 固定强类型决定。
    pub action: SettlementReviewAction,
    /// 客户端稳定操作 ID。
    #[validate(length(min = 1, max = 64, message = "操作ID长度必须在1-64之间"))]
    #[validate(custom(function = "safe_command_id", message = "操作ID格式非法"))]
    pub operation_id: String,
    /// 驳回原因代码；驳回必填，确认必须为空。
    pub reason_code: Option<String>,
    /// 决定说明。
    #[validate(length(max = 512, message = "决定说明长度不能超过512"))]
    pub comment: Option<String>,
}

impl SettlementReviewDecisionData {
    /// 解析复核决定附带的原因代码并执行命令协议校验。
    ///
    /// 驳回必须携带可解析为 [`SettlementReviewRejectReason`] 的原因代码；
    /// 确认不得携带任何原因代码。线上传输字符串的规范化与 allowlist 由
    /// 领域值对象独占，本方法只做协议分支。
    ///
    /// # 参数
    /// 无显式参数（方法接收者为已反序列化的决定载荷）。
    ///
    /// # 返回
    /// 驳回时返回解析后的强类型原因；确认时返回 `None`。
    ///
    /// # 错误
    /// 驳回缺失、非法或未知原因，以及确认携带原因时返回错误。
    ///
    /// # 约束
    /// 不改变线上传输形态；原因语义以领域值对象三元集合为准。
    pub fn parsed_reject_reason(&self) -> Result<Option<SettlementReviewRejectReason>> {
        match self.action {
            SettlementReviewAction::Reject => {
                let raw = self.reason_code.as_deref().ok_or_else(|| {
                    crate::errors::Error::ValidationError("驳回必须携带原因代码".to_string())
                })?;
                Ok(Some(SettlementReviewRejectReason::parse(raw)?))
            }
            SettlementReviewAction::Confirm if self.reason_code.is_some() => Err(
                crate::errors::Error::ValidationError("确认结算不得携带驳回原因代码".to_string()),
            ),
            SettlementReviewAction::Confirm => Ok(None),
        }
    }
}

/// 供应商结算复核唯一强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementReviewCommand {
    /// 当前正式复核任务。
    #[validate(length(min = 1, max = 128, message = "待办ID长度必须在1-128之间"))]
    pub work_item_id: String,
    /// 查询所得任务版本（字符串用于跨端稳定表示）。
    #[validate(custom(function = "non_blank", message = "待办版本不能为空"))]
    pub expected_task_version: String,
    /// 任务冻结的结算主题摘要。
    #[validate(length(equal = 64, message = "主题版本必须为64位"))]
    pub expected_subject_version: String,
    /// 强类型业务决定。
    #[validate(nested)]
    pub decision: SettlementReviewDecisionData,
    /// 正式命令幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 复核决定结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewDecisionStatus {
    /// 已确认结算。
    Confirmed,
    /// 已驳回给经办人。
    Rejected,
}

/// 供应商结算正式复核结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementReviewDecisionResult {
    /// 固定结果状态。
    pub result_status: SettlementReviewDecisionStatus,
    /// 面向用户的稳定说明。
    pub message: String,
    /// 原请求操作 ID。
    pub operation_id: String,
    /// 决定后的结算单投影。
    pub statement: SupplierSettlementStatementView,
    /// 已完成的正式任务。
    pub work_item_id: String,
    /// 固定终态。
    pub work_item_status: WorkItemStatus,
    /// 任务完成后的版本。
    pub task_version: u64,
    /// 确认形成的应付编号；驳回为空。
    pub payable_no: Option<String>,
    /// 确认形成的应付账户；驳回为空。
    pub payable_account_id: Option<String>,
    /// 结算确认追加的含税成本差额；驳回为空。
    pub cost_delta_gross: Option<Amount>,
}
