//! Catalog 修订序号值规则。
//!
//! 不可变修订在同一稳定身份内从 1 单调递增；数据库只负责提供已有序号，
//! 本模块负责确定性地计算下一序号并阻止整数溢出。

use crate::errors::{Error, Result};

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
    revision_nos
        .into_iter()
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| Error::from("修订序号已达上限"))
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
}
