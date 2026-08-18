//! 审批流程定义管理。P3-DEFINITION 填充实现。

use crate::errors::{Error, Result};

/// 拒绝尚未接线的定义管理调用。
///
/// # 错误
/// 始终返回业务逻辑错误，不得创建、发布或退役流程。
pub fn refuse_unwired() -> Result<()> {
    Err(Error::BusinessLogicError(
        "审批流程定义管理尚未接入，已按安全策略拒绝".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::refuse_unwired;
    use crate::errors::Error;

    /// 定义管理占位必须失败关闭，并钉死稳定文案。
    #[test]
    fn definition_placeholder_fails_closed() {
        let Err(Error::BusinessLogicError(message)) = refuse_unwired() else {
            panic!("定义占位必须返回 BusinessLogicError");
        };
        assert_eq!(message, "审批流程定义管理尚未接入，已按安全策略拒绝");
    }
}
