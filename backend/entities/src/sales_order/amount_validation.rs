//! 销售单域共享的表头金额三元组校验。
//!
//! `validate_amount_triple` 同时被工作副本、正式版本与提交校验复用，
//! 因此独立成模块，避免 `revision`/`submission` 反向依赖 `working_copy`。

use crate::errors::{Error, Result};
use crate::money::Amount;

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
