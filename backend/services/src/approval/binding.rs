//! 单据与已发布定义的绑定。P3-ADAPTER-BASE 填充实现。

use crate::errors::{Error, Result};

/// 拒绝尚未接线的绑定调用。
///
/// # 错误
/// 始终返回业务逻辑错误，不得静默跳过绑定或返回默认定义。
pub fn refuse_unwired() -> Result<()> {
    Err(Error::BusinessLogicError(
        "审批定义绑定尚未接入，已按安全策略拒绝".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::refuse_unwired;
    use crate::errors::Error;

    /// 绑定占位必须失败关闭，并钉死稳定文案。
    #[test]
    fn binding_placeholder_fails_closed() {
        let Err(Error::BusinessLogicError(message)) = refuse_unwired() else {
            panic!("绑定占位必须返回 BusinessLogicError");
        };
        assert_eq!(message, "审批定义绑定尚未接入，已按安全策略拒绝");
    }
}
