//! 四资金纠错实体共用的纯规则（§6.11 共同不变量与审批版本）。
//!
//! 原寄宿于 `customer_refund` 并被其余三实体以 `super::customer_refund::`
//! 路径复用；现下沉到同层共享模块，四实体均从此处引用。校验规则与文案不变。

use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};

/// 经办人/复核人标识最大长度。
pub(crate) const ACTOR_MAX_LEN: usize = 128;
/// 纠错单据编号最大长度（退款单号/冲正单号共用）。
pub(crate) const DOCUMENT_NO_MAX_LEN: usize = 64;
/// 原因代码最大长度。
pub(crate) const REASON_CODE_MAX_LEN: usize = 32;
/// 原因文本最大长度。
pub(crate) const REASON_TEXT_MAX_LEN: usize = 512;
/// 创建人标识最大长度。
pub(crate) const CREATOR_ID_MAX_LEN: usize = 128;

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
    validate_exclusive_target(
        original_receipt_id,
        original_receivable_entry_id,
        "原回款与原应收只能指向其一",
        "退款必须指向原回款或原应收",
    )
}

/// 校验两类原事实引用必须且只能选其一（文案由调用方保持原语义）。
///
/// # 参数
/// * `first` - 第一类原事实引用
/// * `second` - 第二类原事实引用
/// * `both_message` - 同时提供时的错误说明
/// * `neither_message` - 均未提供时的错误说明
///
/// # 返回
/// 恰选其一时返回 `Ok(())`。
///
/// # 错误
/// 同时或均未提供时按调用方文案返回错误。
pub(crate) fn validate_exclusive_target<T, U>(
    first: &Option<T>,
    second: &Option<U>,
    both_message: &str,
    neither_message: &str,
) -> Result<()> {
    match (first.is_some(), second.is_some()) {
        (true, true) => Err(Error::from(both_message)),
        (false, false) => Err(Error::from(neither_message)),
        _ => Ok(()),
    }
}

/// 校验纠错金额为正数（零与负数均拒绝，文案由调用方保持原语义）。
pub(crate) fn ensure_positive_amount(amount: Amount, message: &str) -> Result<()> {
    if amount.to_decimal().is_sign_negative() || amount.to_decimal().is_zero() {
        return Err(Error::from(message));
    }
    Ok(())
}

/// 规范化创建人标识（trim/非空/长度，规则与文案四单据一致）。
pub(crate) fn normalize_created_by(created_by: impl Into<String>) -> Result<String> {
    normalize_required_text(created_by.into(), "创建人不能为空", CREATOR_ID_MAX_LEN, "创建人标识过长")
}

/// 递增审批提交版本（checked add，溢出文案四单据一致）。
pub(crate) fn next_approval_version(current: u32) -> Result<u32> {
    current.checked_add(1).ok_or_else(|| Error::from("审批提交版本溢出"))
}

/// 校验单据仍是从未提交审批的初始草稿（文案由调用方保持原语义）。
pub(crate) fn ensure_initial_approval(is_draft: bool, version: u32, message: &str) -> Result<()> {
    if !is_draft || version != 0 {
        return Err(Error::from(message));
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
    fn exclusive_target_keeps_caller_messages() {
        assert!(validate_exclusive_target(&Some(1), &None::<i32>, "同时", "均无").is_ok());
        assert!(validate_exclusive_target(&None::<i32>, &Some(2), "同时", "均无").is_ok());
        assert_eq!(
            validate_exclusive_target(&Some(1), &Some(2), "同时", "均无").unwrap_err().to_string(),
            "同时"
        );
        assert_eq!(
            validate_exclusive_target(&None::<i32>, &None::<i32>, "同时", "均无").unwrap_err().to_string(),
            "均无"
        );
        assert!(validate_original_target(&Some(1), &None::<i32>).is_ok());
    }

    #[test]
    fn positive_amount_rejects_zero_and_negative() {
        assert!(ensure_positive_amount(amount("0.01"), "必须为正数").is_ok());
        assert!(ensure_positive_amount(amount("0.00"), "必须为正数").is_err());
        assert!(ensure_positive_amount(amount("-1.00"), "必须为正数").is_err());
    }

    #[test]
    fn created_by_and_approval_version_helpers() {
        assert_eq!(normalize_created_by(" creator-1 ").unwrap(), "creator-1");
        assert!(normalize_created_by("   ").is_err());
        assert_eq!(next_approval_version(0).unwrap(), 1);
        assert!(next_approval_version(u32::MAX).is_err());
        assert!(ensure_initial_approval(true, 0, "已提交").is_ok());
        assert!(ensure_initial_approval(false, 0, "已提交").is_err());
        assert!(ensure_initial_approval(true, 1, "已提交").is_err());
    }
}
