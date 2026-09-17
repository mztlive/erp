//! 四资金纠错实体共用的经办复核与原事实指向校验（§6.11 共同不变量）。
//!
//! 原寄宿于 `customer_refund` 并被其余三实体以 `super::customer_refund::`
//! 路径复用；现下沉到同层共享模块，四实体均从此处引用。校验规则与文案不变。

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};

/// 经办人/复核人标识最大长度。
pub(crate) const ACTOR_MAX_LEN: usize = 128;

/// 校验财务经办人与复核人分离。
///
/// 规则（数据模型 §6.11 共同不变量）：财务经办人与复核人不得相同。
///
/// # 参数
/// * `handled_by` - 经办人
/// * `reviewed_by` - 复核人
///
/// # 返回
/// 返回规范化后的经办人/复核人。
///
/// # 错误
/// 任一方为空/超长或两者相同时返回错误。
pub(crate) fn validate_actor_pair(handled_by: String, reviewed_by: String) -> Result<(String, String)> {
    let handled_by =
        normalize_required_text(handled_by, "财务经办人不能为空", ACTOR_MAX_LEN, "经办人标识过长")?;
    let reviewed_by =
        normalize_required_text(reviewed_by, "财务复核人不能为空", ACTOR_MAX_LEN, "复核人标识过长")?;
    if handled_by == reviewed_by {
        return Err(Error::from("财务经办人与复核人不得相同"));
    }
    Ok((handled_by, reviewed_by))
}

/// 校验退款原事实二选一。
///
/// 规则（数据模型 §6.11）：退款必须指向「原回款」或「原应收」之一。
///
/// # 参数
/// * `original_receipt_id` - 原回款
/// * `original_receivable_entry_id` - 原应收分录
///
/// # 返回
/// 二选一成立返回 `Ok(())`。
///
/// # 错误
/// 同时或均未提供时返回错误。
pub(crate) fn validate_original_target<T, U>(
    original_receipt_id: &Option<T>,
    original_receivable_entry_id: &Option<U>,
) -> Result<()> {
    match (original_receipt_id.is_some(), original_receivable_entry_id.is_some()) {
        (true, true) => Err(Error::from("原回款与原应收只能指向其一")),
        (false, false) => Err(Error::from("退款必须指向原回款或原应收")),
        _ => Ok(()),
    }
}
