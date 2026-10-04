//! 供应链独立命令回执的组合层持久化；展示日志不承担业务协议。
use entity_core::BaseModel;
use erp_audit::AuditLog;
use erp_supply::command_receipt::repository::SupplyCommandReceiptExt;
pub(super) use erp_supply::command_receipt::{CompletionReceipt, InvestigationReceipt};
use erp_supply::command_receipt::{SupplyCommandReceipt, SupplyCommandResult};
pub(super) use erp_supply::service::supplier_fulfillment::receipt::{
    parse_positive_version, serialized_fingerprint, stable_digest, stable_evidence_id,
    stable_internal_idempotency_key,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 将执行后强类型结果登记到拥有领域的回执，沿用调用方事务。
///
/// # 参数
/// * `db` - 组合根绑定的数据库。
/// * `audit` - 本次成功事件的最小身份与关联号。
/// * `fingerprint` - 原入口规范化载荷指纹。
/// * `key` - 原请求幂等键，仅持久化摘要。
/// * `scope_id` - 原请求定位的对象或正式任务。
/// * `result` - 原入口允许恢复的强类型结果。
/// * `executor` - 与业务事实、事件共用的执行器。
/// # 返回
/// 回执保存成功返回空结果。
/// # 错误
/// 身份、结果或指纹非法及仓储失败时停止提交。
pub(crate) async fn persist_supply_receipt(
    db: &Database,
    audit: &AuditLog,
    fingerprint: &str,
    key: &str,
    scope_id: &str,
    result: SupplyCommandResult,
    executor: &mut dyn Executor,
) -> Result<()> {
    let fingerprint_algorithm = if fingerprint.starts_with("sha256-v1:") {
        "sha256-canonical-v1"
    } else if matches!(&result, SupplyCommandResult::Investigation(_) | SupplyCommandResult::Completion(_)) {
        "sha256-serialized-v1"
    } else {
        "sha256-length-prefixed-v1"
    };
    let receipt = SupplyCommandReceipt {
        base: BaseModel::new(audit.base.id.clone()),
        schema_version: 1,
        actor_id: audit.actor_id.clone(),
        action: audit.action.clone(),
        resource_type: audit.resource_type.clone(),
        resource_id: audit
            .resource_id
            .clone()
            .ok_or_else(|| Error::Internal("命令回执缺少结果定位".to_string()))?,
        scope_id: scope_id.to_string(),
        idempotency_key_hash: stable_digest(key.trim()),
        fingerprint: fingerprint.to_string(),
        fingerprint_algorithm: fingerprint_algorithm.to_string(),
        result,
        audit_event_id: audit.base.id.clone(),
    };
    receipt.validate()?;
    db.supply_command_receipts().create(&receipt, executor).await?;
    Ok(())
}
