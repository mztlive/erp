//! BPM 纯状态引擎入口。P3-RUNTIME 填充实现。
//!
//! 只声明不可调用的稳定类型；不得注册可被业务调用的伪引擎。

use crate::error::{Error, Result};

/// 状态迁移计划。P3 填充字段与构造；P0 仅冻结类型名。
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionPlan;

/// 中性领域事件。P3 填充字段与构造；P0 仅冻结类型名。
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BpmEvent;

/// 拒绝调用尚未接线的引擎入口。
///
/// # 错误
/// 始终返回 [`Error::NotWired`]，不得产生迁移计划或领域事件。
pub fn refuse_unwired() -> Result<TransitionPlan> {
    Err(Error::NotWired)
}

#[cfg(test)]
mod tests {
    use super::refuse_unwired;
    use crate::error::Error;

    /// 引擎占位必须失败关闭。
    #[test]
    fn engine_placeholder_fails_closed() {
        assert_eq!(refuse_unwired(), Err(Error::NotWired));
    }
}
