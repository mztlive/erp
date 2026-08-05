//! `RevisionBase`：不可变修订公共字段（P0-1.4 共享基元任务）。
//!
//! 对应数据模型 4.3「不可变修订」：`revision_no`。修订一经形成不得修改内容，
//! 只允许追加更高序号的新修订；修订正文存放在域内各自的修订表中。

use serde::{Deserialize, Serialize};

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
}
