//! 供应商交接幂等收据身份。

use sha2::{Digest, Sha256};

use crate::dto::handover::{HandoverSupplierCapabilityRequest, HandoverSupplierRequest};
use crate::{Error, Result};

/// 为供应商维护人交接生成不泄露原始幂等键的稳定收据 ID。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `supplier_id` - 供应商
/// * `idempotency_key` - 客户端幂等键
///
/// # 返回
/// 返回审计收据 ID。
///
/// # 错误
/// 无。
pub fn supplier_handover_audit_id(actor_id: &str, supplier_id: &str, idempotency_key: &str) -> String {
    format!(
        "supplier-handover-{}",
        hex::encode(Sha256::digest(format!("{actor_id}|{supplier_id}|{idempotency_key}").as_bytes()))
    )
}

/// 锁定同一幂等键可重放的完整交接请求身份。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `supplier_id` - 供应商
/// * `request` - 交接请求
///
/// # 返回
/// 返回载荷指纹。
///
/// # 错误
/// 请求序列化失败时返回内部错误。
pub fn supplier_handover_fingerprint(
    actor_id: &str,
    supplier_id: &str,
    request: &HandoverSupplierRequest,
) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, supplier_id, request))
        .map_err(|error| Error::Internal(format!("供应商交接命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// 为能力负责人交接生成稳定收据 ID。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `capability_id` - 能力
/// * `idempotency_key` - 客户端幂等键
///
/// # 返回
/// 返回审计收据 ID。
///
/// # 错误
/// 无。
pub fn capability_handover_audit_id(actor_id: &str, capability_id: &str, idempotency_key: &str) -> String {
    format!(
        "supplier-capability-handover-{}",
        hex::encode(Sha256::digest(format!("{actor_id}|{capability_id}|{idempotency_key}").as_bytes()))
    )
}

/// 锁定同一幂等键可重放的能力交接请求身份。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `capability_id` - 能力
/// * `request` - 交接请求
///
/// # 返回
/// 返回载荷指纹。
///
/// # 错误
/// 请求序列化失败时返回内部错误。
pub fn capability_handover_fingerprint(
    actor_id: &str,
    capability_id: &str,
    request: &HandoverSupplierCapabilityRequest,
) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, capability_id, request))
        .map_err(|error| Error::Internal(format!("供应商能力交接命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// 构造交接审计收据的消息体。
///
/// # 参数
/// * `fingerprint` - 完整载荷指纹
/// * `target` - 目标人员
/// * `reason` - 交接原因
///
/// # 返回
/// 返回写入审计收据 `message` 字段的稳定消息体。
///
/// # 错误
/// 无。
pub fn supplier_handover_audit_message(fingerprint: &str, target: &str, reason: &str) -> String {
    format!("command_sha256={fingerprint};target={target};reason={reason}")
}

/// 校验已存交接审计收据是否绑定同一交接指纹。
///
/// # 参数
/// * `message` - 已存审计收据的 `message` 字段
/// * `expected_fingerprint` - 本次回放期望的完整载荷指纹
///
/// # 返回
/// 同一载荷重放时返回 `true`。
///
/// # 错误
/// 无。
pub fn supplier_handover_fingerprint_matches(message: Option<&str>, expected_fingerprint: &str) -> bool {
    let Some(message) = message else {
        return false;
    };
    let exact = format!("command_sha256={expected_fingerprint}");
    message == exact || message.starts_with(&format!("{exact};"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::handover::HandoverSupplierRequest;

    #[test]
    fn handover_receipt_binds_fingerprint_and_rejects_alien_payload() {
        let stored = supplier_handover_audit_message("fp-same", "buyer-b", "轮换");
        assert!(supplier_handover_fingerprint_matches(Some(&stored), "fp-same"));
        assert!(!supplier_handover_fingerprint_matches(Some(&stored), "fp-other"));
        assert!(!supplier_handover_fingerprint_matches(None, "fp-same"));
        let request = HandoverSupplierRequest {
            expected_version: 1,
            target_user_id: "buyer-b".into(),
            target_org_unit_id: None,
            reason: "轮换".into(),
            idempotency_key: "k1".into(),
        };
        let fingerprint = supplier_handover_fingerprint("actor", "sup-1", &request).unwrap();
        assert!(!fingerprint.is_empty());
        assert_eq!(supplier_handover_audit_id("a", "s", "k"), supplier_handover_audit_id("a", "s", "k"));
    }
}
