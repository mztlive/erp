//! 命令的原版本解析与稳定身份；无审计或工作项依赖。
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{Error, Result};
pub fn parse_positive_version(value: &str, field: &str) -> Result<u64> {
    let version = value
        .trim()
        .parse::<u64>()
        .map_err(|_| Error::ValidationError(format!("{field}必须为正整数字符串")))?;
    if version == 0 {
        return Err(Error::ValidationError(format!("{field}必须为正整数字符串")));
    }
    Ok(version)
}

pub fn serialized_fingerprint<T: Serialize>(command: &T) -> Result<String> {
    let bytes = serde_json::to_vec(command)
        .map_err(|error| Error::Internal(format!("命令指纹序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// 由稳定领域命令身份派生证据 ID，保留既有字节算法。
///
/// # 参数
/// * `prefix` - 证据类型前缀。
/// * `command_id` - 稳定领域命令身份，独立于展示事件。
/// # 返回
/// 返回确定性证据 ID。
/// # 错误
/// 无。
pub fn stable_evidence_id(prefix: &str, command_id: &str) -> String {
    format!("{prefix}-{}", stable_digest(command_id))
}

/// 由稳定领域命令身份派生内部幂等键，保留既有字节算法。
///
/// # 参数
/// * `prefix` - 证据类型前缀。
/// * `command_id` - 稳定领域命令身份。
/// # 返回
/// 返回领域内部去重键。
/// # 错误
/// 无。
pub fn stable_internal_idempotency_key(prefix: &str, command_id: &str) -> String {
    format!("{prefix}:{}", stable_digest(command_id))
}

pub fn stable_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
