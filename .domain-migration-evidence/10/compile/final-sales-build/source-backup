//! Stable sales command receipt identities and payload fingerprints.
use crate::dto::sales_order::SubmitSalesOrderRequest;
use crate::{Error, Result};
use sha2::{Digest, Sha256};

/// 为销售提交幂等命令生成不泄露原始幂等键的稳定收据 ID。
pub fn sales_submission_audit_id(actor_id: &str, sales_order_id: &str, idempotency_key: &str) -> String {
    format!(
        "sales-order-submit-{}",
        hex::encode(Sha256::digest(
            format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes()
        ))
    )
}

/// 锁定同一幂等键可重放的完整请求身份。
///
/// # 错误
/// 请求序列化失败时返回内部错误。
pub fn sales_submission_fingerprint(
    actor_id: &str,
    sales_order_id: &str,
    request: &SubmitSalesOrderRequest,
) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, sales_order_id, request))
        .map_err(|error| Error::Internal(format!("销售提交命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// Compute the unchanged sales command identity from its complete original input.
///
/// Errors preserve serialization failure; no persistence or approval binding occurs here.
pub fn sales_order_create_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    let mut digest = Sha256::new();
    for part in [actor_id, idempotency_key.trim()] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sales-order-create-{}", hex::encode(digest.finalize()))
}

/// Compute the unchanged sales command identity from its complete original input.
///
/// Errors preserve serialization failure; no persistence or approval binding occurs here.
pub fn sales_order_create_fingerprint<T: serde::Serialize>(actor_id: &str, request: &T) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, request))
        .map_err(|error| Error::Internal(format!("销售建单命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}
