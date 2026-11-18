//! 供应商退款及留抵对采购处置方案金额的占用规则。

use erp_core::money::Amount;

use super::{SupplierRefund, SupplierRefundStatus};
use crate::{Error, Result};

impl SupplierRefund {
    /// 校验本次金额不超过同采购、同方案、同结算方式的剩余额度。
    /// 未超过剩余额度时返回成功，允许刚好用尽。
    /// `limit` 为采购方案中选定结算方式的总金额；`settlements` 为已读取的处理记录；自身、非占用状态及软删除记录不计入。
    /// 审批中及已过账占用加本次金额超过方案限额时返回冲突。
    pub fn ensure_settlement_limit(&self, limit: Amount, settlements: &[Self]) -> Result<()> {
        let used = settlements
            .iter()
            .filter(|row| {
                row.base.id != self.base.id
                    && !row.base.is_deleted()
                    && SupplierRefundStatus::SETTLEMENT_OCCUPYING.contains(&row.status)
                    && row.source_purchase_order_id == self.source_purchase_order_id
                    && row.settlement_method == self.settlement_method
                    && row.settlement_decision_key == self.settlement_decision_key
            })
            .fold(Amount::zero(), |total, row| total.checked_add(row.amount));

        if self.amount > limit.checked_sub(used) {
            return Err(Error::ConflictError(
                "本次金额超过采购方案剩余可处理金额，审批中的金额也已占用".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entity_core::BaseModel;
    use erp_core::ids::SupplierRefundId;

    use super::super::SupplierSettlementMethod;
    use super::super::tests::data;
    use super::*;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn refund(id: &str, status: SupplierRefundStatus, value: &str) -> SupplierRefund {
        let mut refund = SupplierRefund::new(SupplierRefundId::new(id), data(), "creator").unwrap();
        refund.base = BaseModel { id: id.into(), ..BaseModel::fake() };
        refund.status = status;
        refund.amount = amount(value);
        refund.source_purchase_order_id = Some("purchase-1".into());
        refund.settlement_decision_key = Some("decision-1".into());
        refund
    }

    #[test]
    fn pending_and_posted_use_limit_and_excess_is_rejected() {
        let mut current = refund("current", SupplierRefundStatus::Rejected, "30.00");
        let rows = [
            refund("pending", SupplierRefundStatus::InApproval, "40.01"),
            refund("posted", SupplierRefundStatus::Posted, "29.99"),
        ];
        current.ensure_settlement_limit(amount("100.00"), &rows).unwrap();

        current.amount = amount("30.01");
        assert!(matches!(
            current.ensure_settlement_limit(amount("100.00"), &rows),
            Err(Error::ConflictError(message))
                if message == "本次金额超过采购方案剩余可处理金额，审批中的金额也已占用"
        ));

        current.amount = amount("0.01");
        assert!(current.ensure_settlement_limit(amount("70.00"), &rows).is_err());
    }

    #[test]
    fn draft_rejected_reversed_deleted_and_self_do_not_use_limit() {
        let current = refund("current", SupplierRefundStatus::InApproval, "100.00");
        let mut deleted = refund("deleted", SupplierRefundStatus::Posted, "100.00");
        deleted.base.deleted_at = 1;
        let rows = [
            current.clone(),
            refund("draft", SupplierRefundStatus::Draft, "100.00"),
            refund("rejected", SupplierRefundStatus::Rejected, "100.00"),
            refund("reversed", SupplierRefundStatus::Reversed, "100.00"),
            deleted,
        ];

        current.ensure_settlement_limit(amount("100.00"), &rows).unwrap();
        assert!(current.ensure_settlement_limit(amount("99.99"), &rows).is_err());
    }

    #[test]
    fn other_purchase_decision_and_settlement_method_have_separate_limits() {
        let current = refund("current", SupplierRefundStatus::Draft, "100.00");
        let mut other_purchase = refund("other-purchase", SupplierRefundStatus::Posted, "100.00");
        other_purchase.source_purchase_order_id = Some("purchase-2".into());
        let mut other_decision = refund("other-decision", SupplierRefundStatus::InApproval, "100.00");
        other_decision.settlement_decision_key = Some("decision-2".into());
        let mut credit = refund("credit", SupplierRefundStatus::Posted, "100.00");
        credit.settlement_method = SupplierSettlementMethod::SupplierCredit;

        current.ensure_settlement_limit(amount("100.00"), &[other_purchase, other_decision, credit]).unwrap();
    }
}
