//! 审批业务适配器。P3-ADAPTER-* 填充强类型领域动作。

use crate::errors::{Error, Result};

/// 拒绝尚未接线的业务动作。
///
/// # 错误
/// 始终返回业务逻辑错误，不得返回 Noop 成功或默认审批人。
pub fn refuse_unwired() -> Result<()> {
    Err(Error::BusinessLogicError(
        "审批业务适配器尚未接入，已按安全策略拒绝".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::refuse_unwired;
    use crate::errors::Error;

    /// 业务适配占位必须失败关闭，并钉死稳定文案。
    #[test]
    fn business_adapter_placeholder_fails_closed() {
        let Err(Error::BusinessLogicError(message)) = refuse_unwired() else {
            panic!("业务适配占位必须返回 BusinessLogicError");
        };
        assert_eq!(message, "审批业务适配器尚未接入，已按安全策略拒绝");
    }
}
