//! Catalog 修订序号与生效窗口值规则。
//!
//! 不可变修订在同一稳定身份内从 1 单调递增；数据库只负责提供已有序号，
//! 本模块负责确定性地计算下一序号并阻止整数溢出。商品与 SKU 修订共用
//! 序号下限与生效窗口校验，错误文案保持单一来源。

use erp_core::common::time::BusinessDate;
use erp_core::{Error, Result};

/// 计算一组既有修订序号之后的下一序号。
///
/// # 参数
/// * `revision_nos` - 同一稳定身份下的既有修订序号
///
/// # 返回
/// 空集合返回 `1`；否则返回最大序号加一。
///
/// # 错误
/// 最大序号已经达到 `u32::MAX` 时返回领域错误，禁止回绕或重复使用序号。
pub fn next_revision_no(revision_nos: impl IntoIterator<Item = u32>) -> Result<u32> {
    revision_nos.into_iter().max().unwrap_or(0).checked_add(1).ok_or_else(|| Error::from("修订序号已达上限"))
}

/// 校验修订序号从 1 开始。
///
/// # 参数
/// * `revision_no` - 修订序号
///
/// # 返回
/// 大于等于 1 时返回 `Ok(())`。
///
/// # 错误
/// 为 0 时返回错误。
pub(crate) fn ensure_revision_no(revision_no: u32) -> Result<()> {
    if revision_no == 0 {
        return Err(Error::from("修订序号必须从 1 开始"));
    }
    Ok(())
}

/// 校验生效区间不倒挂。
///
/// # 参数
/// * `effective_from` - 生效开始日
/// * `effective_to` - 生效结束日
///
/// # 返回
/// 结束日晚于开始日（或无限期）时返回 `Ok(())`。
///
/// # 错误
/// 结束日早于或等于开始日时返回错误。
pub(crate) fn ensure_effective_window(
    effective_from: BusinessDate,
    effective_to: Option<BusinessDate>,
) -> Result<()> {
    if let Some(effective_to) = effective_to
        && effective_to <= effective_from
    {
        return Err(Error::from("生效结束日必须晚于生效开始日"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空修订集合从 1 开始，非空集合按最大序号递增且不依赖输入顺序。
    #[test]
    fn next_revision_number_is_deterministic() {
        assert_eq!(next_revision_no([]).unwrap(), 1);
        assert_eq!(next_revision_no([2, 1, 4, 3]).unwrap(), 5);
    }

    /// 最大序号达到整数上限时拒绝回绕。
    #[test]
    fn next_revision_number_rejects_overflow() {
        assert!(next_revision_no([1, u32::MAX]).is_err());
    }

    /// 修订序号 0 被拒绝，文案与商品/SKU 修订实体一致。
    #[test]
    fn ensure_revision_no_rejects_zero_with_stable_message() {
        assert_eq!(ensure_revision_no(0).unwrap_err().to_string(), "修订序号必须从 1 开始");
        assert!(ensure_revision_no(1).is_ok());
    }

    /// 生效结束日必须严格晚于开始日；无限期通过。
    #[test]
    fn ensure_effective_window_rejects_non_increasing_range() {
        let from = BusinessDate::from_ymd(2026, 1, 1).unwrap();
        assert!(ensure_effective_window(from, None).is_ok());
        assert!(ensure_effective_window(from, Some(BusinessDate::from_ymd(2026, 1, 2).unwrap())).is_ok());
        assert_eq!(
            ensure_effective_window(from, Some(from)).unwrap_err().to_string(),
            "生效结束日必须晚于生效开始日"
        );
        assert_eq!(
            ensure_effective_window(from, Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()))
                .unwrap_err()
                .to_string(),
            "生效结束日必须晚于生效开始日"
        );
    }
}
