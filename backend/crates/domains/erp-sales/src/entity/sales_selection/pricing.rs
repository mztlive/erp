//! 选品册金额运算：禁止浮点，溢出整次拒绝。

use erp_core::money::Amount;
use erp_core::{Error, Result};
use rust_decimal::Decimal;

/// 两个金额精确相加。
///
/// # 参数
/// * `left` - 左操作数
/// * `right` - 右操作数
///
/// # 返回
/// 返回可被 `Amount` 精确表达的和。
///
/// # 错误
/// 溢出或小数位无法表达时拒绝，不截断、不按零处理。
pub fn try_add(left: Amount, right: Amount) -> Result<Amount> {
    let sum = left
        .to_decimal()
        .checked_add(right.to_decimal())
        .ok_or_else(|| Error::from("金额合计超出可表示范围"))?;
    Amount::try_from(sum).map_err(|_| Error::from("金额合计超出可表示范围"))
}

/// 金额乘以正整数量。
///
/// # 参数
/// * `unit` - 单价
/// * `quantity` - 份数或件数
///
/// # 返回
/// 返回行金额。
///
/// # 错误
/// 溢出或无法精确表达时拒绝。
pub fn try_mul_u32(unit: Amount, quantity: u32) -> Result<Amount> {
    let product = unit
        .to_decimal()
        .checked_mul(Decimal::from(quantity))
        .ok_or_else(|| Error::from("行金额超出可表示范围"))?;
    Amount::try_from(product).map_err(|_| Error::from("行金额超出可表示范围"))
}

/// 多个金额求和。
///
/// # 参数
/// * `amounts` - 金额序列
///
/// # 返回
/// 空序列返回零；否则返回精确合计。
///
/// # 错误
/// 任一步溢出时拒绝。
pub fn try_sum<I>(amounts: I) -> Result<Amount>
where
    I: IntoIterator<Item = Amount>,
{
    let mut total = Amount::zero();
    for amount in amounts {
        total = try_add(total, amount)?;
    }
    Ok(total)
}

/// 两个金额的绝对差。
///
/// # 参数
/// * `left` - 左操作数
/// * `right` - 右操作数
///
/// # 返回
/// 返回 `|left - right|`。
///
/// # 错误
/// 无。金额类型减法不溢出。
pub fn abs_diff(left: Amount, right: Amount) -> Amount {
    if left >= right { left.checked_sub(right) } else { right.checked_sub(left) }
}

/// 判断售价是否落在目标金额 ± 容差内（含边界）。
///
/// # 参数
/// * `price` - 套餐售价
/// * `target` - 档位目标金额
/// * `tolerance` - 容差
///
/// # 返回
/// 落在区间内返回 `true`。下限按 0 处理。
///
/// # 错误
/// 目标加容差溢出时返回错误。
pub fn price_in_tier(price: Amount, target: Amount, tolerance: Amount) -> Result<bool> {
    let lower = if tolerance >= target { Amount::zero() } else { target.checked_sub(tolerance) };
    let upper = try_add(target, tolerance)?;
    Ok(price >= lower && price <= upper)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::Amount;

    use super::{abs_diff, price_in_tier, try_add, try_mul_u32, try_sum};

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).expect("测试金额必须合法")
    }

    #[test]
    fn quantity_line_matches_unit_times_copies() {
        let line = try_mul_u32(amount("12.50"), 3).unwrap();
        assert_eq!(line, amount("37.50"));
    }

    #[test]
    fn sum_rejects_overflow() {
        let huge = Amount::try_from(rust_decimal::Decimal::MAX).unwrap_or(amount("9999999999999999.99"));
        let result = try_add(huge, huge);
        assert!(result.is_err());
    }

    #[test]
    fn empty_sum_is_zero() {
        assert_eq!(try_sum(std::iter::empty()).unwrap(), Amount::zero());
    }

    #[test]
    fn tier_includes_boundaries_and_floors_at_zero() {
        assert!(price_in_tier(amount("98.00"), amount("100.00"), amount("2.00")).unwrap());
        assert!(price_in_tier(amount("102.00"), amount("100.00"), amount("2.00")).unwrap());
        assert!(!price_in_tier(amount("97.99"), amount("100.00"), amount("2.00")).unwrap());
        assert!(price_in_tier(amount("0.00"), amount("1.00"), amount("5.00")).unwrap());
        assert_eq!(abs_diff(amount("198.00"), amount("200.00")), amount("2.00"));
    }
}
