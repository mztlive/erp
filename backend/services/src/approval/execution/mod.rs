//! 审批运行编排。P3-RUNTIME 填充实现。
//!
//! 不得提供可被业务调用的伪引擎、Noop 领域动作或默认审批人。

use crate::errors::{Error, Result};

/// 拒绝尚未接线的运行编排调用。
///
/// # 错误
/// 始终返回业务逻辑错误。
pub fn refuse_unwired() -> Result<()> {
    Err(Error::BusinessLogicError(
        "审批运行编排尚未接入，已按安全策略拒绝".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::refuse_unwired;
    use crate::errors::Error;

    /// 运行编排占位必须失败关闭，并钉死稳定文案。
    #[test]
    fn execution_placeholder_fails_closed() {
        let Err(Error::BusinessLogicError(message)) = refuse_unwired() else {
            panic!("运行编排占位必须返回 BusinessLogicError");
        };
        assert_eq!(message, "审批运行编排尚未接入，已按安全策略拒绝");
    }
}
