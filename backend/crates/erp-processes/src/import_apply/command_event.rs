//! W18 导入强命令安全事件元数据；结果恢复由导入领域拥有。
use application_core::{AuditActor, StructuredCommandReceipt};
use erp_audit::{AuditAction, BusinessEventContent, BusinessEventContext, BusinessEventResult};

use crate::Result;

/// 在事务开始前验证动作及认证身份，稳定命令 ID 只作事件关联。
/// # 参数
/// * `actor` - 当前认证操作人。
/// * `action` - 固定动作代码。
/// * `resource` - 固定资源类型。
/// * `label` - 当前动作中文名称。
/// * `identity` - 完整规范化命令身份。
/// # 返回
/// 返回与原稳定命令 ID 关联的新事件上下文。
/// # 错误
/// 身份或元数据无效时在业务写入前拒绝执行。
pub(super) fn import_event_context(
    actor: &AuditActor,
    action: &'static str,
    resource: &'static str,
    label: &'static str,
    identity: &StructuredCommandReceipt,
) -> Result<BusinessEventContext> {
    Ok(BusinessEventContext::new(
        actor.clone(),
        AuditAction { code: action, resource_type: resource, label, version: 1, allowed_fields: &[] },
    )?
    .with_command_id(Some(identity.command_id.clone()))?)
}

/// 结果只记录当前动作成功及正式目标，不写入任意导入载荷或解释文本。
/// # 参数
/// * `target_id` - 当前命令正式结果目标。
/// # 返回
/// 返回允许持久化的最小成功投影。
///
/// # 错误
/// 不返回错误。
pub(super) fn import_event_content(target_id: String) -> BusinessEventContent {
    BusinessEventContent {
        target_id,
        target_number: None,
        result: BusinessEventResult::Succeeded,
        field_changes: Vec::new(),
        facts: Vec::new(),
    }
}
