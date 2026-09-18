//! 采购单主状态、财务审核状态与独立进度（数据模型 §6.6 / §7.4）。

use erp_core::common::state::DocumentState;
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::types::status_display;

/// 采购单状态（数据模型 §6.6/§7.4，审批合同 §4.4.2 已收敛）。
///
/// 目标状态机：`DRAFT → IN_APPROVAL → EFFECTIVE → PARTIALLY_EXECUTED →
/// COMPLETED`；撤回回到 `DRAFT`；草稿且无下游事实可作废（`DRAFT → VOIDED`）。
/// `PENDING_FINANCE_REVIEW` 仅为旧数据反序列化保留，新写入不得进入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseOrderStatus {
    /// 草稿。
    Draft,
    /// 待财务审核。
    #[serde(rename = "PENDING_FINANCE_REVIEW")]
    PendingFinanceReview,
    /// 已生效。
    Effective,
    /// 部分执行。
    #[serde(rename = "PARTIALLY_EXECUTED")]
    PartiallyExecuted,
    /// 已完成。
    Completed,
    /// 已作废。
    Voided,
    /// 审批中。
    #[serde(rename = "IN_APPROVAL")]
    InApproval,
}

status_display!(PurchaseOrderStatus, {
    Draft => ("草稿", "DRAFT"),
    PendingFinanceReview => ("待财务审核", "PENDING_FINANCE_REVIEW"),
    Effective => ("已生效", "EFFECTIVE"),
    PartiallyExecuted => ("部分执行", "PARTIALLY_EXECUTED"),
    Completed => ("已完成", "COMPLETED"),
    Voided => ("已作废", "VOIDED"),
    InApproval => ("审批中", "IN_APPROVAL"),
});

impl PurchaseOrderStatus {
    /// 判断当前状态是否允许发起采购变更。
    ///
    /// # 返回
    /// 已生效或部分执行时返回 `true`，其余状态返回 `false`。
    pub fn allows_change(self) -> bool {
        matches!(self, Self::Effective | Self::PartiallyExecuted)
    }
}

impl DocumentState for PurchaseOrderStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::InApproval, Self::Voided],
            Self::InApproval => &[Self::Effective, Self::Draft],
            Self::PendingFinanceReview => &[],
            Self::Effective => &[Self::PartiallyExecuted],
            Self::PartiallyExecuted => &[Self::Completed],
            Self::Completed => &[],
            Self::Voided => &[],
        }
    }
}

/// 采购财务审核状态（§6.6：待审核、通过、驳回；独立于主状态审核轨）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseReviewStatus {
    /// 待审核。
    Pending,
    /// 通过。
    Approved,
    /// 驳回。
    Rejected,
}

status_display!(PurchaseReviewStatus, {
    Pending => ("待审核", "PENDING"),
    Approved => ("通过", "APPROVED"),
    Rejected => ("驳回", "REJECTED"),
});

/// 独立进度（§6.6 付款/收票/履约三条独立进度：未开始、部分、已完成）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProgressStatus {
    /// 未开始。
    None,
    /// 部分完成。
    Partial,
    /// 已完成。
    Completed,
}

status_display!(ProgressStatus, {
    None => ("未开始", "NONE"),
    Partial => ("部分", "PARTIAL"),
    Completed => ("已完成", "COMPLETED"),
});
