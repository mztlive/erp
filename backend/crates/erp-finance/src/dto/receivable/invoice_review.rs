//! 开票审批额度构成；本次占用与其他申请分列，已执行金额不重复扣减。
use erp_core::money::Amount;
use serde::Serialize;

use crate::entity::receivable::{InvoiceRequestStatus, SalesInvoiceRequest};

/// 当前应收的申请占用与剩余可申请额度，负差额独立展示。
#[derive(Debug, Serialize)]
pub struct InvoiceReviewAmounts {
    pub current_reserved: Amount,
    pub other_pending: Amount,
    pub other_approved: Amount,
    pub available: Amount,
    pub shortage: Amount,
}

impl InvoiceReviewAmounts {
    /// 从同一应收的有效申请计算额度；已开票金额已由开放额度扣除。
    /// # 参数
    /// `open` 为未开票额度，`current_id` 为本次申请，`requests` 为同一应收申请。
    /// # 返回
    /// 本次占用、其他占用、可申请余额及超占金额。
    /// # 错误
    /// 无；金额按领域定点数计算。
    pub fn from_requests(open: Amount, current_id: &str, requests: &[SalesInvoiceRequest]) -> Self {
        Self::from_entries(
            open,
            current_id,
            requests.iter().map(|row| (row.base.id.as_str(), row.status, row.remaining())),
        )
    }

    fn from_entries<'a>(
        open: Amount,
        current_id: &str,
        entries: impl Iterator<Item = (&'a str, InvoiceRequestStatus, Amount)>,
    ) -> Self {
        let mut view = Self {
            current_reserved: Amount::zero(),
            other_pending: Amount::zero(),
            other_approved: Amount::zero(),
            available: Amount::zero(),
            shortage: Amount::zero(),
        };
        for (id, status, remaining) in entries {
            if !matches!(status, InvoiceRequestStatus::InApproval | InvoiceRequestStatus::Approved) {
                continue;
            }
            if id == current_id {
                view.current_reserved = view.current_reserved.checked_add(remaining);
            } else if status == InvoiceRequestStatus::InApproval {
                view.other_pending = view.other_pending.checked_add(remaining);
            } else {
                view.other_approved = view.other_approved.checked_add(remaining);
            }
        }
        let balance = open
            .checked_sub(view.current_reserved)
            .checked_sub(view.other_pending)
            .checked_sub(view.other_approved);
        view.available = balance.max(Amount::zero());
        view.shortage = Amount::zero().checked_sub(balance).max(Amount::zero());
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn amount(value: &str) -> Amount {
        value.parse().unwrap()
    }

    #[test]
    fn separates_current_reservation_and_only_counts_unexecuted_amounts() {
        let rows = [
            ("current", InvoiceRequestStatus::InApproval, amount("165")),
            ("pending", InvoiceRequestStatus::InApproval, amount("20.01")),
            ("approved", InvoiceRequestStatus::Approved, amount("30")),
            ("draft", InvoiceRequestStatus::Draft, amount("900")),
            ("done", InvoiceRequestStatus::Completed, amount("0")),
        ];
        let view = InvoiceReviewAmounts::from_entries(amount("300"), "current", rows.into_iter());
        assert_eq!(view.current_reserved, amount("165"));
        assert_eq!(view.other_pending, amount("20.01"));
        assert_eq!(view.other_approved, amount("30"));
        assert_eq!(view.available, amount("84.99"));
        assert_eq!(view.shortage, Amount::zero());
    }

    #[test]
    fn exposes_shortage_and_preserves_zero_without_negative_available() {
        let view = InvoiceReviewAmounts::from_entries(
            amount("100"),
            "current",
            [("current", InvoiceRequestStatus::Approved, amount("120.01"))].into_iter(),
        );
        assert_eq!(view.available, Amount::zero());
        assert_eq!(view.shortage, amount("20.01"));
        let empty = InvoiceReviewAmounts::from_entries(Amount::zero(), "missing", [].into_iter());
        assert_eq!(empty.current_reserved, Amount::zero());
        assert_eq!(empty.available, Amount::zero());
    }
}
