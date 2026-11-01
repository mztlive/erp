//! 采购与供应商确认的退货款处置意图，不代表资金或余额已入账。
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 采购提交的不可变处置方案。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReturnSettlementDecision {
    pub idempotency_key: String,
    pub sales_revision_id: String,
    pub target_gross: Amount,
    pub refund_amount: Amount,
    pub credit_amount: Amount,
    pub supplier_confirmation: String,
}
impl ReturnSettlementDecision {
    /// 判断当前采购是否可交接财务办理。
    /// # 参数
    /// * `decision` - 采购已确认方案。
    /// * `revision` / `target` - 当前销售依据及采购目标。
    /// * `pending` / `physical_complete` - 已登记退货未结案，且应退实物已全部确认。
    /// # 返回
    /// 方案与当前依据匹配且实物已完成时返回真。
    /// # 错误
    /// 无。
    pub fn ready_for_finance(
        decision: Option<&Self>,
        revision: &str,
        target: Amount,
        pending: bool,
        physical_complete: bool,
    ) -> bool {
        pending
            && physical_complete
            && decision
                .is_some_and(|value| value.sales_revision_id == revision && value.target_gross == target)
    }

    /// 计算本次需处理的款票差额。
    /// # 参数
    /// * `total` / `target` - 当前净额及目标。
    /// # 返回
    /// 大于目标的部分，否则为零。
    /// # 错误
    /// 无。
    pub fn excess(total: Amount, target: Amount) -> Amount {
        if total > target { total.checked_sub(target) } else { Amount::zero() }
    }

    /// 核对本次方案与当前来源及多付金额完全一致。
    /// # 参数
    /// * `revision` / `target` - 当前来源版本与调整目标。
    /// * `excess` - 当前需退款或转留抵的金额。
    /// # 返回
    /// 金额守恒且依据完整时成功。
    /// # 错误
    /// 版本过期、负数、金额不守恒或缺少依据时拒绝。
    pub fn validate(&self, revision: &str, target: Amount, excess: Amount) -> Result<()> {
        if self.sales_revision_id != revision || self.target_gross != target {
            return Err(Error::from("销售依据或调整金额已变化，请重新核对处置方案"));
        }
        if self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 128
            || self.supplier_confirmation.trim().is_empty()
            || self.supplier_confirmation.len() > 1500
        {
            return Err(Error::from("请填写供应商确认依据后提交"));
        }
        if self.refund_amount < Amount::zero()
            || self.credit_amount < Amount::zero()
            || self.refund_amount.to_decimal().checked_add(self.credit_amount.to_decimal())
                != Some(excess.to_decimal())
        {
            return Err(Error::from("退款与留抵金额合计必须等于本次待处理金额"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
    fn decision() -> ReturnSettlementDecision {
        ReturnSettlementDecision {
            idempotency_key: "key".into(),
            sales_revision_id: "r2".into(),
            target_gross: amount("1000"),
            refund_amount: amount("75"),
            credit_amount: amount("100"),
            supplier_confirmation: "供应商确认退货及留抵".into(),
        }
    }
    #[test]
    fn finance_handoff_requires_current_decision_and_confirmed_physical_return() {
        let plan = decision();
        assert!(ReturnSettlementDecision::ready_for_finance(Some(&plan), "r2", amount("1000"), true, true));
        for (decision, revision, pending, returned) in [
            (None, "r2", true, true),
            (Some(&plan), "r3", true, true),
            (Some(&plan), "r2", false, true),
            (Some(&plan), "r2", true, false),
        ] {
            assert!(!ReturnSettlementDecision::ready_for_finance(
                decision,
                revision,
                amount("1000"),
                pending,
                returned
            ));
        }
        assert_eq!(ReturnSettlementDecision::excess(amount("1175"), amount("1000")), amount("175"));
        assert_eq!(ReturnSettlementDecision::excess(amount("900"), amount("1000")), amount("0"));
    }
    #[test]
    fn split_cash_and_credit_must_conserve_excess() {
        let mut plan = decision();
        assert!(plan.validate("r2", amount("1000"), amount("175")).is_ok());
        plan.credit_amount = amount("101");
        assert!(plan.validate("r2", amount("1000"), amount("175")).is_err());
        plan.credit_amount = amount("-1");
        assert!(plan.validate("r2", amount("1000"), amount("74")).is_err());
    }
    #[test]
    fn changed_basis_or_missing_evidence_is_rejected() {
        let mut plan = decision();
        assert!(plan.validate("r3", amount("1000"), amount("175")).is_err());
        assert!(plan.validate("r2", amount("900"), amount("175")).is_err());
        plan.supplier_confirmation.clear();
        assert!(plan.validate("r2", amount("1000"), amount("175")).is_err());
    }
}
