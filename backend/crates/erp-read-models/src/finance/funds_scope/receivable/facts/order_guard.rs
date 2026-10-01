//! 核销批量读取的金额顺序证明；极值不能证明时保留原单账查询及运算。

use erp_core::money::Amount;
use rust_decimal::Decimal;

/// Decimal 可用的最大无符号 96 位尾数。
const MAX_MANTISSA: i128 = (1_i128 << 96) - 1;

/// 证明这组金额在任意正反排列下均可按两位小数精确折叠。
///
/// # 参数
/// * `amounts` - 单个子账的全部核销金额；包含正向与反向，不按动作合并
///
/// # 返回
/// 所有金额可精确换成分币整数且绝对系数总和不超过 96 位尾数时返回 true。
/// 此界覆盖任意中间净额，保证 Decimal 无需降到两位以下或舍入。
/// 空集合及零金额返回 true；false 要求调用方回读原单账查询并按原流运算。
///
/// # 错误
/// 不返回错误；换算不精确、整数运算溢出或超过尾数上界均返回 false。
pub(super) fn order_independent(amounts: impl IntoIterator<Item = Amount>) -> bool {
    let mut absolute_total = 0_i128;
    for amount in amounts {
        let Some(coefficient) = cent_coefficient(amount.to_decimal()).and_then(i128::checked_abs) else {
            return false;
        };
        let Some(next) = absolute_total.checked_add(coefficient) else {
            return false;
        };
        if next > MAX_MANTISSA {
            return false;
        }
        absolute_total = next;
    }
    true
}

/// 以 primitive checked 运算精确换成两位系数，不能用 Decimal 加法证明其上界。
fn cent_coefficient(decimal: Decimal) -> Option<i128> {
    let mantissa = decimal.mantissa();
    let scale = decimal.scale();
    if scale <= 2 {
        return mantissa.checked_mul(10_i128.checked_pow(2 - scale)?);
    }
    let divisor = 10_i128.checked_pow(scale - 2)?;
    if mantissa.checked_rem(divisor)? != 0 {
        return None;
    }
    mantissa.checked_div(divisor)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    /// 使用真实金额解析路径，避免测试中以自定义系数替代金额合同。
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    #[test]
    /// 常规正反金额、负值、零和空集合在任意排列下均保持精确净额。
    fn ordinary_signed_amounts_are_order_independent() {
        assert!(order_independent([]));
        assert!(order_independent([Amount::zero(), amount("-0.00")]));
        let rows = [amount("12.34"), amount("-1"), amount("1")];
        assert!(order_independent(rows));
        for permutation in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            let result =
                permutation.into_iter().fold(Amount::zero(), |sum, index| sum.checked_add(rows[index]));
            assert_eq!(result, amount("12.34"));
        }
        assert!(order_independent([amount("-12.34"), amount("0.01")]));
    }

    #[test]
    /// Decimal::MAX 和交错正反极值在执行任何金额运算前要求原流回退。
    fn decimal_maximum_and_cancelling_extremes_require_original_order() {
        let maximum = Amount::try_from(Decimal::MAX).unwrap();
        assert!(!order_independent([maximum]));
        assert!(!order_independent([maximum, amount("-1"), amount("1")]));
        assert!(!order_independent([Amount::try_from(Decimal::MIN).unwrap()]));
        assert!(!order_independent([maximum, amount("-79228162514264337593543950335")]));
    }

    #[test]
    /// 恰好 96 位的两位系数允许，任何非零附加绝对金额都必须回退。
    fn exact_cent_mantissa_boundary_is_inclusive() {
        let boundary = amount("792281625142643375935439503.35");
        let negative = amount("-792281625142643375935439503.35");
        assert_eq!(cent_coefficient(boundary.to_decimal()), Some(MAX_MANTISSA));
        assert!(order_independent([boundary, Amount::zero()]));
        assert!(order_independent([negative]));
        assert!(!order_independent([boundary, amount("-0.01")]));
        assert!(!order_independent([boundary, amount("1"), amount("-1")]));
    }

    #[test]
    /// 真实金额的尾零和归一化 scale 0/1/2 保持相同分币值，不依赖 Decimal 求和。
    fn normalized_amount_scales_and_trailing_zeros_keep_exact_cent_values() {
        for (value, expected, normalized_scale) in
            [("123.0000", 12300, 0), ("12.3000", 1230, 1), ("12.3400", 1234, 2), ("-12.3000", -1230, 1)]
        {
            let original = amount(value);
            let normalized = Amount::try_from(original.to_decimal().normalize()).unwrap();
            assert_eq!(normalized.to_decimal().scale(), normalized_scale);
            assert_eq!(cent_coefficient(original.to_decimal()), Some(expected));
            assert_eq!(cent_coefficient(normalized.to_decimal()), Some(expected));
            assert!(order_independent([original, normalized]));
        }
        assert_eq!(cent_coefficient(Decimal::from_str("12.3400").unwrap()), Some(1234));
        assert_eq!(cent_coefficient(Decimal::from_str("12.3401").unwrap()), None);
        assert_eq!(cent_coefficient(Decimal::from_str("-0.001").unwrap()), None);
    }
}
