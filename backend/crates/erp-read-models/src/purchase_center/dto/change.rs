//! 采购变更单视图。

use super::*;

/// 采购变更单视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeOrderView {
    /// 变更单主键。
    pub id: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 变更原因。
    pub reason: String,
    /// 状态。
    pub status: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 生效后形成的新采购版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}
