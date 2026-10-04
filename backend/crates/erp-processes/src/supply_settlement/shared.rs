//! 结算独立命令的稳定身份；不读取或解析展示日志。
use erp_supply::service::supplier_settlement::shared::digest_parts;

const COMMAND_RECEIPT_PREFIX: &str = "supplier-settlement-command-";
/// 生成不暴露原始幂等键的稳定领域命令 ID。
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
