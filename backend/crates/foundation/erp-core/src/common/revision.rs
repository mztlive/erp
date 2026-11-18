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
    /// 为领域即将写入的不可变修订附上已分配的版本号；版本号如何分配不由此方法决定。
    pub fn new(revision_no: u32) -> Self {
        Self { revision_no }
    }

    /// 准备新增修订时在最新版本上加一，首版从一开始。达到 `u32` 上限时拒绝，避免回绕复用历史版本号。
    pub fn next_revision_no(latest: Option<u32>) -> Result<u32> {
        latest.unwrap_or(0).checked_add(1).ok_or_else(|| Error::from("修订序号已达上限"))
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
