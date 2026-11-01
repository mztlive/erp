//! 供应商交接幂等收据身份。

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::dto::handover::{HandoverSupplierCapabilityRequest, HandoverSupplierRequest};
use crate::{Error, Result};

/// 序列化载荷并计算指纹（erp-supplier-009）。
///
/// # 参数
/// * `label` - 序列化失败时的命令标签
/// * `payload` - 待指纹的载荷元组
///
/// # 返回
/// 返回载荷 SHA-256 hex 指纹。
///
/// # 错误
/// 请求序列化失败时返回内部错误。
fn handover_fingerprint(label: &str, payload: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|error| Error::Internal(format!("{label}序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
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
    handover_fingerprint("供应商交接命令", &(actor_id, supplier_id, request))
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
    handover_fingerprint("供应商能力交接命令", &(actor_id, capability_id, request))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_bind_original_request_fields_and_target_types() {
        let request = HandoverSupplierRequest {
            expected_version: 1,
            target_user_id: "buyer-b".into(),
            target_org_unit_id: None,
            reason: "轮换".into(),
            idempotency_key: "key-1".into(),
        };
        let fp = supplier_handover_fingerprint("actor", "supplier-1", &request).unwrap();
        assert_eq!(fp.len(), 64);
        assert_eq!(fp, supplier_handover_fingerprint("actor", "supplier-1", &request).unwrap());
        let mut changed = request.clone();
        changed.expected_version = 2;
        assert_ne!(fp, supplier_handover_fingerprint("actor", "supplier-1", &changed).unwrap());
        let capability = HandoverSupplierCapabilityRequest {
            expected_version: 1,
            target_user_id: "buyer-b".into(),
            reason: "轮换".into(),
            idempotency_key: "key-1".into(),
        };
        assert_ne!(fp, capability_handover_fingerprint("actor", "supplier-1", &capability).unwrap());
    }
}
