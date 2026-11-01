//! 批量售价的精确整数运算；只在最终转分时进行非负数半入。

use erp_core::money::Amount;
use rust_decimal::Decimal;

use crate::{Error, Result};

/// 按共同整数尺度相加，只在最终结果转分时舍入。
/// # 参数
/// `source` 为非负接口报价，`amount` 为非负固定加价额，均已通过类型精度校验。
/// # 返回
/// 按非负数半入规则保留两位小数的公司售价。
/// # 错误
/// 计算结果无法准确表示为公司金额时拒绝。
pub(super) fn add(source: Decimal, amount: Decimal) -> Result<Amount> {
    let (source, amount) = (source.normalize(), amount.normalize());
    let scale = source.scale().max(amount.scale());
    let source = source.mantissa().checked_mul(10_i128.pow(scale - source.scale())).ok_or_else(overflow)?;
    let amount = amount.mantissa().checked_mul(10_i128.pow(scale - amount.scale())).ok_or_else(overflow)?;
    let value = source.checked_add(amount).ok_or_else(overflow)?;
    rounded_amount(value, 1, scale)
}

/// 用整数商余数完成比例乘法，禁止 Decimal 中途降低精度。
/// # 参数
/// `source` 为非负接口报价，`rate` 为非负百分数，均已通过四位有效小数校验。
/// # 返回
/// 完整乘法后仅进行一次半入的公司售价。
/// # 错误
/// 精确结果超出公司金额可表示范围时拒绝。
pub(super) fn percentage(source: Decimal, rate: Decimal) -> Result<Amount> {
    let (source, rate) = (source.normalize(), rate.normalize());
    let factor = 100_i128
        .checked_mul(10_i128.pow(rate.scale()))
        .and_then(|base| base.checked_add(rate.mantissa()))
        .ok_or_else(overflow)?;
    rounded_amount(source.mantissa(), factor, source.scale() + rate.scale() + 2)
}

fn rounded_amount(left: i128, right: i128, scale: u32) -> Result<Amount> {
    let cents = if scale <= 2 {
        left.checked_mul(right).and_then(|value| value.checked_mul(10_i128.pow(2 - scale)))
    } else {
        let divisor = 10_i128.pow(scale - 2);
        // 分解后再乘，避免一个本可按分表示的结果因未舍入乘积超过 i128 而误报溢出。
        // 输入小数位最多 4 + 4 + 2，tail < 10^8，因子至多 96 位加 10^6。
        let whole = (left / divisor).checked_mul(right).ok_or_else(overflow)?;
        let tail = (left % divisor).checked_mul(right).ok_or_else(overflow)?;
        whole
            .checked_add(tail / divisor)
            .and_then(|value| value.checked_add(i128::from((tail % divisor) * 2 >= divisor)))
    }
    .ok_or_else(overflow)?;
    cent_amount(cents)
}

fn cent_amount(mut cents: i128) -> Result<Amount> {
    let mut scale = 2;
    loop {
        if let Ok(value) = Decimal::try_from_i128_with_scale(cents, scale) {
            return Ok(Amount::try_from(value)?);
        }
        // 只去掉不影响金额的尾零；禁止为容纳尾数再舍入一次。
        if scale == 0 || cents % 10 != 0 {
            return Err(overflow());
        }
        cents /= 10;
        scale -= 1;
    }
}

fn overflow() -> Error {
    Error::ValidationError("批量售价计算超出金额范围".into())
}
