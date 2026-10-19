//! 采购付款安排只描述未申请余额，不授予付款资格。
use erp_core::money::Amount;
use serde::Serialize;

use super::purchase_order::PurchaseOrderStatus;

/// 按采购状态、余额与付款阻断得到的下一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentArrangementState {
    Inactive,
    Available,
    Blocked,
    Pending,
    Settled,
}
impl PaymentArrangementState {
    /// 根据同一快照中的采购及应付事实确定安排状态。
    /// # 参数
    /// 采购状态、剩余应付、有效申请占用、来源付款阻断。
    /// # 返回
    /// 当前安排状态；申请占用不视为已付。
    /// # 错误
    /// 无。
    pub fn resolve(status: PurchaseOrderStatus, open: Amount, reserved: Amount, blocked: bool) -> Self {
        if !matches!(
            status,
            PurchaseOrderStatus::Effective
                | PurchaseOrderStatus::PartiallyExecuted
                | PurchaseOrderStatus::Completed
        ) {
            return Self::Inactive;
        }
        if open <= Amount::zero() {
            return Self::Settled;
        }
        if open <= reserved {
            return Self::Pending;
        }
        if blocked { Self::Blocked } else { Self::Available }
    }
    /// 是否需要保留采购经办人的安排责任。
    /// # 参数
    /// 无。
    /// # 返回
    /// 有未申请余额时返回真，阻断不消除跟进责任。
    /// # 错误
    /// 无。
    pub fn needs_task(self) -> bool {
        matches!(self, Self::Available | Self::Blocked)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrangement_tracks_available_balance_not_payment_authority() {
        let zero = Amount::zero();
        let total = "100".parse().unwrap();
        let partial = "30".parse().unwrap();
        let status = PurchaseOrderStatus::Effective;
        assert_eq!(
            PaymentArrangementState::resolve(status, total, zero, false),
            PaymentArrangementState::Available
        );
        assert!(PaymentArrangementState::resolve(status, total, partial, false).needs_task());
        assert_eq!(
            PaymentArrangementState::resolve(status, total, total, false),
            PaymentArrangementState::Pending
        );
        assert_eq!(
            PaymentArrangementState::resolve(status, total, zero, true),
            PaymentArrangementState::Blocked
        );
        assert_eq!(
            PaymentArrangementState::resolve(status, zero, zero, true),
            PaymentArrangementState::Settled
        );
        for status in [PurchaseOrderStatus::Draft, PurchaseOrderStatus::InApproval] {
            assert_eq!(
                PaymentArrangementState::resolve(status, total, zero, false),
                PaymentArrangementState::Inactive
            );
        }
        // 驳回、撤回或关闭释放占用后恢复可申请；部分实付不消除余款责任。
        assert!(PaymentArrangementState::resolve(status, total, zero, false).needs_task());
        assert!(PaymentArrangementState::resolve(status, partial, zero, false).needs_task());
    }
}
