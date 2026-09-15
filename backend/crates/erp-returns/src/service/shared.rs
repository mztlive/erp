//! 退货与资金纠错的稳定命令编号、来源版本及已过账前置规则。
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// 由操作者与幂等键生成不泄露原键的稳定纠错单号。
pub fn return_command_no(prefix: &str, actor_id: &str, idempotency_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(format!("{actor_id}|{}", idempotency_key.trim()).as_bytes()));
    format!("{prefix}-{}", &digest[..8])
}

/// 校验纠错命令的原资金事实仍为同一版本且已经过账。
///
/// # 参数
/// * `actual_version` - 事务内重读所得版本
/// * `expected_version` - 命令准备阶段读取的版本
/// * `is_posted` - 原事实是否处于已过账状态
/// * `not_posted_message` - 当前纠错类型对应的业务说明
///
/// # 返回
/// 版本和状态均满足时返回成功。
///
/// # 错误
/// 版本变化返回冲突；原事实未过账返回业务规则错误。
pub fn ensure_posted_source(
    actual_version: u64,
    expected_version: u64,
    is_posted: bool,
    not_posted_message: &str,
) -> Result<()> {
    if actual_version != expected_version {
        return Err(Error::ConflictError("原资金记录已变化，请刷新后重试".to_string()));
    }
    if !is_posted {
        return Err(Error::BusinessLogicError(not_posted_message.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_posted_source;
    use crate::Error;

    #[test]
    fn correction_source_requires_same_posted_fact() {
        assert!(ensure_posted_source(3, 3, true, "必须已过账").is_ok());
        assert!(matches!(ensure_posted_source(4, 3, true, "必须已过账"), Err(Error::ConflictError(_))));
        assert!(matches!(ensure_posted_source(3, 3, false, "必须已过账"), Err(Error::BusinessLogicError(_))));
    }
}
