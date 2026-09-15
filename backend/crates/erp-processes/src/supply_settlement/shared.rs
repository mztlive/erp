//! 结算根流程的审计回执身份与解析。
use erp_supply::service::supplier_settlement::shared::digest_parts;

use crate::{Error, Result};
const COMMAND_RECEIPT_PREFIX: &str = "supplier-settlement-command-";
pub(super) const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
/// 生成不暴露原始幂等键的稳定审计收据 ID。
pub(super) fn command_audit_id(
    actor_id: &str,
    action: &str,
    resource_id: &str,
    idempotency_key: &str,
) -> String {
    let digest = digest_parts(&[
        actor_id.to_string(),
        action.to_string(),
        resource_id.to_string(),
        idempotency_key.to_string(),
    ]);
    format!("{COMMAND_RECEIPT_PREFIX}{digest}")
}

/// 提取审计消息中的命令指纹与结果载荷。
pub(super) fn receipt_result<'a>(
    message: &'a str,
    expected_fingerprint: &str,
    command_name: &str,
) -> Result<&'a str> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal(format!("{command_name}幂等收据格式非法")))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(format!("幂等键已用于不同的{command_name}命令")));
    }
    Ok(result)
}

/// 解析幂等收据中的正整数版本。
pub(super) fn parse_receipt_number(value: &str, field: &str) -> Result<u64> {
    let value = value.parse::<u64>().map_err(|_| Error::Internal(format!("结算命令收据{field}非法")))?;
    if value == 0 {
        return Err(Error::Internal(format!("结算命令收据{field}非法")));
    }
    Ok(value)
}

/// 校验幂等收据仍指向同一成功业务资源。
pub(super) fn ensure_audit_resource(audit: &erp_audit::AuditLog, resource_id: &str) -> Result<()> {
    if !audit.success || audit.resource_id.as_deref() != Some(resource_id) {
        return Err(Error::ConflictError("幂等收据与当前业务资源不一致".to_string()));
    }
    Ok(())
}
