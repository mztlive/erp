//! 商城明细成本按支付来源金额比例分摊计划（INT-E05）。
//!
//! 承接原 Service `build_cost_assessments` 中的比例分摊、尾差归尾、含税拆分与
//! 金额守恒；不生成 ID、不读写数据库。Service 仅将分摊结果转为成本评估、
//! `CostEntry`/`CostAllocation` 并持久化。

use std::str::FromStr;

use crate::errors::{Error, Result};
use crate::money::{round_to_cent, Amount, Rate};

/// 单条支付来源分摊腿（含税/不含税金额已守恒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostShareLeg {
    /// 分摊含税（或税前口径下的）成本。
    pub gross_amount: Amount,
    /// 分摊不含税成本。
    pub net_amount: Amount,
    /// 分摊税额；不含税成本时为精确零。
    pub tax_amount: Amount,
    /// 进项税率；不含税成本时为 `None`。
    pub input_tax_rate: Option<Rate>,
    /// 是否吸收该明细的舍入尾差（仅最后一条为 `true`）。
    pub rounding_residual_flag: bool,
}

/// 明细成本按支付来源金额比例分摊的计划结果。
///
/// 规则：
/// - 非末腿：`gross = round_to_cent(cost_total × payment_amount / item_paid)`；
/// - 末腿：吸收尾差，`gross = cost_total − 已分摊合计`（不得为负）；
/// - 含税：`tax = round_to_cent(gross × rate)`，`net = gross − tax`；
/// - 不含税：`net = gross`，`tax = 0`，不写进项税率；
/// - 全部分摊腿 `gross` 合计精确等于 `cost_total`，且每腿 `gross = net + tax`。
///
/// 乘除加减一律走 `Decimal::checked_*`，溢出或尾差为负时返回 [`Error::LogicError`]，不 panic。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostSharePlan {
    legs: Vec<CostShareLeg>,
}

impl CostSharePlan {
    /// 判断明细成本快照是否足以产出 `ACTUAL` 分摊。
    ///
    /// # 参数
    /// * `cost_snapshot_total` - 明细供货成本合计
    /// * `cost_tax_inclusion` - 成本含税标识
    /// * `cost_input_tax_rate` - 含税成本时的进项税率
    ///
    /// # 返回
    /// 成本合计与含税标识齐全，且含税时进项税率也齐全时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 与历史 Service `has_actual` 判定一致；不完整数据由调用方走 `NONE` 评估。
    pub fn has_actual_cost(
        cost_snapshot_total: Option<Amount>,
        cost_tax_inclusion: Option<bool>,
        cost_input_tax_rate: Option<Rate>,
    ) -> bool {
        cost_snapshot_total.is_some()
            && cost_tax_inclusion.is_some()
            && (!cost_tax_inclusion.unwrap_or(false) || cost_input_tax_rate.is_some())
    }

    /// 按支付来源金额比例分摊明细成本，末腿吸收分币尾差。
    ///
    /// # 参数
    /// * `cost_total` - 明细成本合计（守恒基准）
    /// * `item_paid` - 明细实付（比例分母）
    /// * `payment_amounts` - 已按来源序号稳定排序的分摊实付；空切片产出空计划
    /// * `tax_inclusion` - 成本是否含税
    /// * `input_tax_rate` - 含税时必填的进项税率；不含税时忽略
    ///
    /// # 返回
    /// 返回与 `payment_amounts` 一一对应、总额守恒且各腿非负的分摊计划。
    ///
    /// # 错误
    /// 含税但缺税率、多腿且实付为零、定点乘除加减溢出、舍入后金额非法、
    /// 或末腿尾差为负（非末腿舍入已超过成本合计）时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 纯内存确定性计算：不生成 ID、不访问数据库、不 panic。
    pub fn share(
        cost_total: Amount,
        item_paid: Amount,
        payment_amounts: &[Amount],
        tax_inclusion: bool,
        input_tax_rate: Option<Rate>,
    ) -> Result<Self> {
        if tax_inclusion && input_tax_rate.is_none() {
            return Err(Error::from("含税成本必须提供进项税率"));
        }
        if payment_amounts.is_empty() {
            return Ok(Self { legs: Vec::new() });
        }
        if payment_amounts.len() > 1 && item_paid.to_decimal().is_zero() {
            return Err(Error::from("明细实付为零时无法按多来源比例分摊成本"));
        }

        let mut legs = Vec::with_capacity(payment_amounts.len());
        let mut accrued = zero_amount();
        let count = payment_amounts.len();
        for (index, payment_amount) in payment_amounts.iter().enumerate() {
            let is_last = index + 1 == count;
            let gross = if is_last {
                residual_gross(cost_total, accrued)?
            } else {
                proportional_gross(cost_total, *payment_amount, item_paid)?
            };
            accrued = checked_add_amount(accrued, gross)?;
            let (net, tax, rate) = split_tax(gross, tax_inclusion, input_tax_rate)?;
            legs.push(CostShareLeg {
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                input_tax_rate: rate,
                rounding_residual_flag: is_last,
            });
        }
        Ok(Self { legs })
    }

    /// 返回按输入顺序排列的分摊腿。
    ///
    /// # 返回
    /// 返回分摊腿切片，供 Service 组装评估与成本事实。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn legs(&self) -> &[CostShareLeg] {
        &self.legs
    }

    /// 返回全部分摊腿含税成本合计。
    ///
    /// # 返回
    /// 返回 `gross_amount` 精确合计；构造成功的计划合计等于 `cost_total`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn total_gross(&self) -> Amount {
        self.legs
            .iter()
            .fold(zero_amount(), |sum, leg| sum.checked_add(leg.gross_amount))
    }
}

/// 非末腿按支付比例计算分摊含税成本。
///
/// # 参数
/// * `cost_total` - 明细成本合计
/// * `payment_amount` - 本腿分摊实付
/// * `item_paid` - 明细实付（分母）
///
/// # 返回
/// 返回舍入到分的分摊金额。
///
/// # 错误
/// 乘除溢出或舍入后金额非法时返回 [`Error::LogicError`]。
fn proportional_gross(cost_total: Amount, payment_amount: Amount, item_paid: Amount) -> Result<Amount> {
    let product = cost_total
        .to_decimal()
        .checked_mul(payment_amount.to_decimal())
        .ok_or_else(|| Error::from("成本分摊乘积溢出"))?;
    let quotient = product
        .checked_div(item_paid.to_decimal())
        .ok_or_else(|| Error::from("成本分摊除法溢出"))?;
    Amount::try_from(round_to_cent(quotient)).map_err(|error| Error::from(error.to_string()))
}

/// 末腿吸收尾差；拒绝负尾差。
///
/// # 参数
/// * `cost_total` - 明细成本合计
/// * `accrued` - 已分摊非末腿合计
///
/// # 返回
/// 返回非负尾差金额。
///
/// # 错误
/// 减法溢出、尾差为负或金额非法时返回 [`Error::LogicError`]。
fn residual_gross(cost_total: Amount, accrued: Amount) -> Result<Amount> {
    let residual = cost_total
        .to_decimal()
        .checked_sub(accrued.to_decimal())
        .ok_or_else(|| Error::from("成本分摊尾差计算溢出"))?;
    if residual.is_sign_negative() {
        return Err(Error::from("成本分摊尾差为负，非末腿舍入已超过成本合计"));
    }
    Amount::try_from(residual).map_err(|error| Error::from(error.to_string()))
}

/// 按含税标识拆分税额。
///
/// # 参数
/// * `gross` - 分摊含税（或税前）成本
/// * `tax_inclusion` - 是否含税
/// * `input_tax_rate` - 含税进项税率
///
/// # 返回
/// 返回 `(net, tax, input_rate)`。
///
/// # 错误
/// 乘积溢出、舍入后税额非法或净额溢出时返回错误。
fn split_tax(
    gross: Amount,
    tax_inclusion: bool,
    input_tax_rate: Option<Rate>,
) -> Result<(Amount, Amount, Option<Rate>)> {
    if !tax_inclusion {
        return Ok((gross, zero_amount(), None));
    }
    let rate = input_tax_rate.ok_or_else(|| Error::from("含税成本必须提供进项税率"))?;
    let tax_product = gross
        .to_decimal()
        .checked_mul(rate.to_decimal())
        .ok_or_else(|| Error::from("税额乘积溢出"))?;
    let tax = Amount::try_from(round_to_cent(tax_product)).map_err(|error| Error::from(error.to_string()))?;
    let net = checked_sub_amount(gross, tax)?;
    Ok((net, tax, Some(rate)))
}

/// 精确相加两个金额；溢出时失败关闭。
///
/// # 参数
/// * `left` - 加数
/// * `right` - 加数
///
/// # 返回
/// 返回精确和。
///
/// # 错误
/// 定点加法溢出或结果超出金额精度时返回 [`Error::LogicError`]。
fn checked_add_amount(left: Amount, right: Amount) -> Result<Amount> {
    let sum = left
        .to_decimal()
        .checked_add(right.to_decimal())
        .ok_or_else(|| Error::from("成本分摊金额合计溢出"))?;
    Amount::try_from(sum).map_err(|_| Error::from("成本分摊金额合计溢出"))
}

/// 精确相减两个金额；溢出时失败关闭。
///
/// # 参数
/// * `left` - 被减数
/// * `right` - 减数
///
/// # 返回
/// 返回精确差。
///
/// # 错误
/// 定点减法溢出或结果超出金额精度时返回 [`Error::LogicError`]。
fn checked_sub_amount(left: Amount, right: Amount) -> Result<Amount> {
    let diff = left
        .to_decimal()
        .checked_sub(right.to_decimal())
        .ok_or_else(|| Error::from("成本分摊净额计算溢出"))?;
    Amount::try_from(diff).map_err(|_| Error::from("成本分摊净额计算溢出"))
}

/// 返回固定零金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

#[cfg(test)]
mod tests {
    use super::{zero_amount, CostSharePlan};
    use crate::money::{Amount, Rate};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    #[test]
    fn has_actual_cost_requires_complete_tax_fields() {
        assert!(!CostSharePlan::has_actual_cost(None, Some(false), None));
        assert!(!CostSharePlan::has_actual_cost(Some(amount("10.00")), None, None));
        assert!(!CostSharePlan::has_actual_cost(
            Some(amount("10.00")),
            Some(true),
            None
        ));
        assert!(CostSharePlan::has_actual_cost(
            Some(amount("10.00")),
            Some(false),
            None
        ));
        assert!(CostSharePlan::has_actual_cost(
            Some(amount("10.00")),
            Some(true),
            Some(rate("0.130000"))
        ));
    }

    #[test]
    fn zero_payment_legs_yield_empty_plan() {
        let plan = CostSharePlan::share(amount("10.00"), amount("20.00"), &[], false, None).unwrap();
        assert!(plan.legs().is_empty());
        assert_eq!(plan.total_gross(), zero_amount());
    }

    #[test]
    fn single_leg_takes_full_total_without_tax() {
        let plan = CostSharePlan::share(
            amount("10.00"),
            amount("10.00"),
            &[amount("10.00")],
            false,
            Some(rate("0.130000")),
        )
        .unwrap();
        assert_eq!(plan.legs().len(), 1);
        let leg = plan.legs()[0];
        assert_eq!(leg.gross_amount, amount("10.00"));
        assert_eq!(leg.net_amount, amount("10.00"));
        assert_eq!(leg.tax_amount, zero_amount());
        assert!(leg.input_tax_rate.is_none());
        assert!(leg.rounding_residual_flag);
        assert_eq!(plan.total_gross(), amount("10.00"));
    }

    #[test]
    fn multi_leg_applies_proportional_share_and_residual_to_last() {
        // 10.00 × 1/3 = 3.333... → 3.33；第二腿同样 3.33；末腿吸收 3.34。
        let plan = CostSharePlan::share(
            amount("10.00"),
            amount("3.00"),
            &[amount("1.00"), amount("1.00"), amount("1.00")],
            false,
            None,
        )
        .unwrap();
        assert_eq!(
            plan.legs()
                .iter()
                .map(|leg| leg.gross_amount.to_string())
                .collect::<Vec<_>>(),
            vec!["3.33", "3.33", "3.34"]
        );
        assert_eq!(
            plan.legs()
                .iter()
                .map(|leg| leg.rounding_residual_flag)
                .collect::<Vec<_>>(),
            vec![false, false, true]
        );
        assert_eq!(plan.total_gross(), amount("10.00"));
    }

    #[test]
    fn tax_inclusive_split_conserves_gross_net_tax_per_leg() {
        let plan = CostSharePlan::share(
            amount("11.30"),
            amount("100.00"),
            &[amount("40.00"), amount("60.00")],
            true,
            Some(rate("0.130000")),
        )
        .unwrap();
        assert_eq!(plan.total_gross(), amount("11.30"));
        for leg in plan.legs() {
            assert_eq!(
                leg.gross_amount,
                leg.net_amount.checked_add(leg.tax_amount),
                "每腿必须守恒 gross = net + tax"
            );
            assert_eq!(leg.input_tax_rate, Some(rate("0.130000")));
        }
        // 11.30 × 0.4 = 4.52；末腿 6.78。
        assert_eq!(plan.legs()[0].gross_amount, amount("4.52"));
        assert_eq!(plan.legs()[1].gross_amount, amount("6.78"));
    }

    #[test]
    fn input_order_is_preserved() {
        let plan = CostSharePlan::share(
            amount("9.00"),
            amount("9.00"),
            &[amount("2.00"), amount("3.00"), amount("4.00")],
            false,
            None,
        )
        .unwrap();
        assert_eq!(
            plan.legs()
                .iter()
                .map(|leg| leg.gross_amount.to_string())
                .collect::<Vec<_>>(),
            vec!["2.00", "3.00", "4.00"]
        );
    }

    #[test]
    fn tax_inclusive_without_rate_is_rejected() {
        let err = CostSharePlan::share(amount("10.00"), amount("10.00"), &[amount("10.00")], true, None)
            .unwrap_err();
        assert!(err.to_string().contains("进项税率"));
    }

    #[test]
    fn zero_paid_with_multiple_legs_is_rejected() {
        let err = CostSharePlan::share(
            amount("10.00"),
            amount("0.00"),
            &[amount("0.00"), amount("0.00")],
            false,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("实付为零"));
    }

    #[test]
    fn zero_paid_single_leg_takes_full_residual() {
        let plan =
            CostSharePlan::share(amount("5.55"), amount("0.00"), &[amount("0.00")], false, None).unwrap();
        assert_eq!(plan.legs()[0].gross_amount, amount("5.55"));
        assert!(plan.legs()[0].rounding_residual_flag);
    }

    #[test]
    fn multi_leg_rounding_overshoot_rejects_negative_residual() {
        // 0.06 × 0.01 / 0.10 = 0.006 → 0.01；九次非末腿合计 0.09 > 0.06。
        let payments = vec![amount("0.01"); 10];
        let err = CostSharePlan::share(amount("0.06"), amount("0.10"), &payments, false, None).unwrap_err();
        assert!(err.to_string().contains("尾差为负"));
    }

    #[test]
    fn arithmetic_overflow_returns_err_without_panic() {
        let max = Amount::try_from(Decimal::MAX).unwrap();
        // 非末腿：cost_total × payment_amount 溢出。
        let err = CostSharePlan::share(max, amount("1.00"), &[max, amount("1.00")], false, None).unwrap_err();
        assert!(err.to_string().contains("溢出"));

        // 含税拆分：gross × rate 溢出（rate>1 仅用于溢出夹具）。
        let err = CostSharePlan::share(
            max,
            amount("1.00"),
            &[amount("1.00")],
            true,
            Some(rate("2.000000")),
        )
        .unwrap_err();
        assert!(err.to_string().contains("溢出"));
    }
}
