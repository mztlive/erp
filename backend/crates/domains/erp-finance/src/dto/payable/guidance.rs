//! 按采购冻结条款与有效付款生成的只读付款建议，不构成额外付款授权。

use erp_core::money::Amount;
use serde::Serialize;

/// 付款作业的依据；尾款日期不得由先款比例推断。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentGuidanceView {
    /// 可读付款条件。
    pub term_label: String,
    /// 当前生效采购版本含税金额。
    pub purchase_total: Amount,
    /// 有效已付款净核销金额。
    pub paid_total: Amount,
    /// 是否启用先款条件。
    pub prepay_gate: bool,
    /// 履约前累计付款门槛；历史门槛缺失时为空。
    pub required_prepayment: Option<Amount>,
    /// 当前未满足的预付款金额。
    pub prepayment_gap: Option<Amount>,
    /// 可预填的本次金额；尾款条件或历史依据不明确时为空。
    pub suggested_amount: Option<Amount>,
}

impl PaymentGuidanceView {
    /// 从冻结采购依据及当前财务余额装配付款建议。
    ///
    /// # 参数
    /// * `term_label` - 可读条款名称。
    /// * `purchase_total` - 采购生效版本含税总额。
    /// * `paid_total` - 有效净付款。
    /// * `open_total` - 该应付可核销余额。
    /// * `prepay_gate` - 是否为先款条件。
    /// * `required_prepayment` - 已计算的冻结门槛，缺失时为空。
    ///
    /// # 返回
    /// 先款按缺口建议，缺口已满足但余额未清时不建议自动付尾款。
    ///
    /// # 错误
    /// 不返回错误；建议始终限制在非负开放余额内。
    pub fn from_requirement(
        term_label: String,
        purchase_total: Amount,
        paid_total: Amount,
        open_total: Amount,
        prepay_gate: bool,
        required_prepayment: Option<Amount>,
    ) -> Self {
        let zero = Amount::zero();
        let prepayment_gap = required_prepayment.map(|required| required.checked_sub(paid_total).max(zero));
        let suggested_amount = if !prepay_gate || open_total <= zero {
            Some(open_total.max(zero))
        } else {
            prepayment_gap.filter(|gap| *gap > zero).map(|gap| gap.min(open_total))
        };
        Self {
            term_label,
            purchase_total,
            paid_total,
            prepay_gate,
            required_prepayment,
            prepayment_gap,
            suggested_amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guidance(paid: &str, open: &str, required: Option<&str>, gate: bool) -> PaymentGuidanceView {
        PaymentGuidanceView::from_requirement(
            "先款 50%".into(),
            "60".parse().unwrap(),
            paid.parse().unwrap(),
            open.parse().unwrap(),
            gate,
            required.map(|v| v.parse().unwrap()),
        )
    }

    #[test]
    fn prepayment_suggests_only_the_unpaid_gap() {
        for (paid, open, expected) in [("0", "60", "30"), ("10", "50", "20"), ("5", "55", "25")] {
            let view = guidance(paid, open, Some("30"), true);
            assert_eq!(view.suggested_amount, Some(expected.parse().unwrap()));
        }
    }

    #[test]
    fn satisfied_or_missing_threshold_does_not_prefill_balance() {
        for paid in ["30", "40"] {
            let view = guidance(paid, "20", Some("30"), true);
            assert_eq!(view.prepayment_gap, Some(Amount::zero()));
            assert_eq!(view.suggested_amount, None);
        }
        assert_eq!(guidance("0", "60", None, true).suggested_amount, None);
    }

    #[test]
    fn suggestion_is_capped_and_non_prepay_keeps_open_balance() {
        assert_eq!(guidance("0", "20", Some("30"), true).suggested_amount, Some("20".parse().unwrap()));
        assert_eq!(guidance("10", "50", None, false).suggested_amount, Some("50".parse().unwrap()));
        assert_eq!(guidance("60", "0", Some("30"), true).suggested_amount, Some(Amount::zero()));
    }
}
