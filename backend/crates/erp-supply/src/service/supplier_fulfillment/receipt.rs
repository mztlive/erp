//! 命令的原版本解析与稳定身份；无审计或工作项依赖。
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{Error, Result};
/// 把文本解析为正整数版本。
///
/// # 参数
/// * `value` - 待解析文本；前后空白会被去掉。
/// * `field` - 失败信息中的字段名。
///
/// # 返回
/// 返回大于零的版本。
///
/// # 错误
/// 文本不是整数或值为零时返回 `ValidationError`。
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

/// 计算命令 JSON 字节的 SHA-256 指纹。
///
/// # 参数
/// * `command` - 可序列化的命令。
///
/// # 返回
/// 返回十六进制摘要。
///
/// # 错误
/// JSON 序列化失败时返回 `Internal`。
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

/// 计算文本 UTF-8 字节的 SHA-256 摘要。
///
/// # 参数
/// * `value` - 参与摘要的文本。
///
/// # 返回
/// 返回十六进制摘要。
///
/// # 错误
/// 不返回错误。
pub fn stable_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
