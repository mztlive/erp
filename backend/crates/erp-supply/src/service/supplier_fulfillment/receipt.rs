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

pub fn stable_evidence_id(prefix: &str, audit_id: &str) -> String {
    format!("{prefix}-{}", stable_digest(audit_id))
}

pub fn stable_internal_idempotency_key(prefix: &str, audit_id: &str) -> String {
    format!("{prefix}:{}", stable_digest(audit_id))
}

pub fn stable_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
