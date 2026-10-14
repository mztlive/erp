//! 销售单域共享的表头金额三元组校验。
//!
//! `validate_amount_triple` 与 `sum_line_amounts` 同时被工作副本、正式版本与提交复用，
//! 因此独立成模块，避免 `revision`/`submission` 反向依赖 `working_copy`。

use std::str::FromStr;

use erp_core::money::Amount;
use erp_core::{Error, Result};

/// 汇总已经逐行舍入的金额三元组。
///
/// # 参数
/// * `amounts` - 行含税额、不含税额、税额组成的迭代器
///
/// # 返回
/// 返回 `(含税合计, 不含税合计, 税额合计)`；空集合返回保留两位小数的 `0.00`。
///
/// # 错误
/// 不返回错误。金额值对象负责保持精度和范围。
///
/// # Panics
/// 静态字面量 `0.00` 无法解析为金额时 panic。该字面量由代码固定，用于构造零金额。
pub(crate) fn sum_line_amounts(
    amounts: impl IntoIterator<Item = (Amount, Amount, Amount)>,
) -> (Amount, Amount, Amount) {
    let zero = Amount::from_str("0.00").expect("静态零金额必须合法");
    amounts.into_iter().fold((zero, zero, zero), |totals, line| {
        (totals.0.checked_add(line.0), totals.1.checked_add(line.1), totals.2.checked_add(line.2))
    })
}

/// 校验表头金额三元组恒等式（§4.2 规则 2：表头只汇总已舍入的行金额）。
///
/// # 参数
/// * `gross_amount` - 含税合计
/// * `net_amount` - 不含税合计
/// * `tax_amount` - 税额合计
///
/// # 返回
/// 恒等式成立时返回 `Ok(())`。
///
/// # 错误
/// `gross != net + tax` 时返回错误。
pub(crate) fn validate_amount_triple(
    gross_amount: Amount,
    net_amount: Amount,
    tax_amount: Amount,
) -> Result<()> {
    if gross_amount.to_decimal() != net_amount.to_decimal() + tax_amount.to_decimal() {
        return Err(Error::from("表头金额必须满足 gross = net + tax"));
    }
    Ok(())
}

/// 校验销售提交表头金额：恒等式成立且含税合计大于 0（§4.4.4：不得只含赠品明细）。
///
/// # 参数
/// * `gross_amount` - 含税合计
/// * `net_amount` - 不含税合计
/// * `tax_amount` - 税额合计
///
/// # 返回
/// 校验通过时返回 `Ok(())`。
///
/// # 错误
/// 恒等式不成立或含税合计不大于 0 时返回错误。
pub(crate) fn validate_submission_amounts(
    gross_amount: Amount,
    net_amount: Amount,
    tax_amount: Amount,
) -> Result<()> {
    validate_amount_triple(gross_amount, net_amount, tax_amount)?;
    if gross_amount.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("销售单含税金额必须大于 0，不能只有赠品明细"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    #[test]
    fn submission_amounts_require_positive_gross() {
        assert!(validate_submission_amounts(amount("106.00"), amount("100.00"), amount("6.00")).is_ok());
        let err = validate_submission_amounts(amount("0"), amount("0"), amount("0")).unwrap_err();
        assert!(err.to_string().contains("不能只有赠品明细"));
        assert!(validate_submission_amounts(amount("10.00"), amount("9.00"), amount("0.50")).is_err());
    }
}
