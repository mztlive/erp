//! `RevisionBase`：不可变修订公共字段（P0-1.4 共享基元任务）。
//!
//! 对应数据模型 4.3「不可变修订」：`revision_no`。修订一经形成不得修改内容，
//! 只允许追加更高序号的新修订；修订正文存放在域内各自的修订表中。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

/// 不可变修订的公共字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionBase {
    /// 聚合内从 1 递增的修订序号（数据模型 4.1）。
    pub revision_no: u32,
}

impl RevisionBase {
    /// 创建修订公共字段。
    ///
    /// # 参数
    /// * `revision_no` - 修订序号（同一稳定对象内从 1 递增）
    ///
    /// # 返回
    /// 返回修订公共字段实例。
    pub fn new(revision_no: u32) -> Self {
        Self { revision_no }
    }

    /// 由仓储返回的最新修订号计算下一序号。
    ///
    /// # 参数
    /// * `latest` - 当前聚合内最大修订号；没有历史时为 `None`
    ///
    /// # 返回
    /// 无历史返回 `1`；否则返回最大序号加一。
    ///
    /// # 错误
    /// 当前最大序号已为 `u32::MAX` 时返回错误，禁止 panic 或回绕。
    ///
    /// # 关键业务约束
    /// 本方法只做受检后继；并发唯一性仍由唯一索引或 CAS 保证。
    pub fn next_revision_no(latest: Option<u32>) -> Result<u32> {
        latest
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::from("修订序号已达上限"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_serde_roundtrip() {
        let base = RevisionBase::new(3);
        assert_eq!(base.revision_no, 3);

        let json = serde_json::to_string(&base).unwrap();
        let back: RevisionBase = serde_json::from_str(&json).unwrap();
        assert_eq!(back, base);
    }

    #[test]
    fn next_revision_no_starts_at_one_and_increments() {
        assert_eq!(RevisionBase::next_revision_no(None).unwrap(), 1);
        assert_eq!(RevisionBase::next_revision_no(Some(0)).unwrap(), 1);
        assert_eq!(RevisionBase::next_revision_no(Some(1)).unwrap(), 2);
        assert_eq!(RevisionBase::next_revision_no(Some(41)).unwrap(), 42);
    }

    #[test]
    fn next_revision_no_rejects_overflow_without_wrap() {
        let error = RevisionBase::next_revision_no(Some(u32::MAX)).unwrap_err();
        assert_eq!(error.to_string(), "修订序号已达上限");
    }
}
