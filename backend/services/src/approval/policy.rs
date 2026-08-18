//! 审批政策注册。P3-DEFINITION 填充穷尽政策。

use crate::errors::{Error, Result};

/// 拒绝尚未接线的政策查询。
///
/// # 错误
/// 始终返回业务逻辑错误，不得返回默认政策或默认审批人。
pub fn refuse_unwired() -> Result<()> {
    Err(Error::BusinessLogicError(
        "审批政策尚未接入，已按安全策略拒绝".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::refuse_unwired;
    use crate::errors::Error;

    /// 政策占位必须失败关闭，并钉死稳定文案。
    #[test]
    fn policy_placeholder_fails_closed() {
        let Err(Error::BusinessLogicError(message)) = refuse_unwired() else {
            panic!("政策占位必须返回 BusinessLogicError");
        };
        assert_eq!(message, "审批政策尚未接入，已按安全策略拒绝");
    }
}
