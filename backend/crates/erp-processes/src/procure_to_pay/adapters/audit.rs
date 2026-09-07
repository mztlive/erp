//! 审计记录到采购幂等回放消费事实的显式投影。
use erp_audit::AuditLog;
use erp_procurement::entity::facts::AuditReceiptFact;
/// 只提取采购身份、指纹和结果校验所需字段；不改变解码顺序或错误。
pub(crate) fn audit_receipt_fact(audit: &AuditLog) -> AuditReceiptFact {
    AuditReceiptFact {
        success: audit.success,
        actor_id: audit.actor_id.clone(),
        action: audit.action.clone(),
        resource_type: audit.resource_type.clone(),
        resource_id: audit.resource_id.clone(),
        message: audit.message.clone(),
    }
}
