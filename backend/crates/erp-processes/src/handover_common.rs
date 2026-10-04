//! 交接命令的规范化、目标组织检查和安全中文事件事实。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{
    AuditAction, AuditCode, AuditFact, AuditField, AuditFieldKind, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult,
};
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 去空白后的幂等键；空白时拒绝。
///
/// # 参数
/// * `raw` - 请求携带的原始幂等键
///
/// # 返回
/// 返回去空白后的幂等键。
///
/// # 错误
/// 去空白后为空时返回校验错误。
///
/// # 关键业务约束
/// 与既有交接入口保持同一空键文案，不得补默认键。
pub(crate) fn trimmed_idempotency_key(raw: &str) -> Result<String> {
    let key = raw.trim().to_string();
    if key.is_empty() {
        return Err(Error::ValidationError("幂等键不能为空".into()));
    }
    Ok(key)
}

/// 交接事件只记录明确的已完成结果，不序列化原因、请求或人员资料。
pub(crate) const HANDOVER_AUDIT_FIELDS: &[AuditField] = &[
    AuditField {
        code: "handover_result",
        label: "交接结果",
        kind: AuditFieldKind::Code(&[AuditCode { code: "completed", label: "已交接" }]),
    },
    AuditField { code: "responsibility", label: "责任人", kind: AuditFieldKind::Changed },
    AuditField { code: "business_org", label: "业务组织", kind: AuditFieldKind::Changed },
];

/// 由实际业务执行形成的白名单事实，不携带自由文本或人员敏感资料。
pub(crate) struct HandoverEventFacts {
    pub target_id: String,
    pub target_number: Option<String>,
    pub responsibility_changed: bool,
    pub organization_changed: bool,
}

/// 预先验证明确登记的交接动作及命令关联。
///
/// # 参数
/// * `actor` - 认证操作人。
/// * `command` - 独立回执的稳定命令身份。
/// * `action` - 调用方明确登记的领域动作。
/// # 返回
/// 返回提交前使用的安全事件上下文。
/// # 错误
/// 静态元数据或身份非法时拒绝。
pub(crate) fn handover_context(
    actor: &AuditActor,
    command: &CommandReceipt,
    action: AuditAction,
) -> Result<BusinessEventContext> {
    Ok(BusinessEventContext::new(actor.clone(), action)?
        .with_command_id(Some(command.id().to_string()))?
        .with_target(command.scope_id().map(str::to_string), None)?)
}

/// 提供执行后明确的交接完成事实，回放分支不调用本函数。
///
/// # 参数
/// * `value` - 实际目标、业务编号及允许记录的真实变更标记。
/// # 返回
/// 返回安全的成功事件投影。
/// # 错误
/// 无；提交前由统一审计边界校验。
pub(crate) fn handover_content(value: HandoverEventFacts) -> BusinessEventContent {
    let mut facts = vec![AuditFact {
        field: "handover_result".into(),
        value: AuditValue::Code { code: "completed".into(), label: "已交接".into() },
    }];
    if value.responsibility_changed {
        facts.push(AuditFact { field: "responsibility".into(), value: AuditValue::Changed });
    }
    if value.organization_changed {
        facts.push(AuditFact { field: "business_org".into(), value: AuditValue::Changed });
    }
    BusinessEventContent {
        target_id: value.target_id,
        target_number: value.target_number,
        result: BusinessEventResult::Succeeded,
        field_changes: Vec::new(),
        facts,
    }
}

/// 校验显式目标组织整条路径均启用。
///
/// # 参数
/// * `db` - 目标数据库
/// * `target_org` - 可选目标组织
/// * `executor` - 调用方执行器
///
/// # 返回
/// 未指定组织或路径全部启用时成功。
///
/// # 错误
/// 目标组织停用时拒绝。
///
/// # 关键业务约束
/// 只做启用检查，不提供任何范围授权；需拒绝公司组织的调用方先自检。
pub(crate) async fn ensure_org_enabled(
    db: &Database,
    target_org: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let state = OrganizationRepository::new(db).state(executor).await?;
    let tree = OrgTree::new(&state.units)?;
    let path = tree.path(org)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("目标业务组织已停用".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_key_trims_and_rejects_blank() {
        assert_eq!(trimmed_idempotency_key("  key-1  ").unwrap(), "key-1");
        assert!(trimmed_idempotency_key("").is_err());
        assert!(trimmed_idempotency_key("   ").is_err());
    }

    #[test]
    fn handover_event_keeps_typed_completion_and_command_without_machine_message() {
        let actor = AuditActor::new("actor".into(), "maintainer".into(), erp_core::AccountKind::Admin);
        let command = CommandReceipt::from_resource_parts(
            "handover-",
            "actor",
            "product.handover",
            "product",
            "product-1",
            "key",
            ["payload".into()],
        )
        .unwrap();
        let context = handover_context(
            &actor,
            &command,
            AuditAction {
                code: "product.handover",
                resource_type: "product",
                label: "商品交接",
                version: 1,
                allowed_fields: HANDOVER_AUDIT_FIELDS,
            },
        )
        .unwrap();
        let log = context
            .log(handover_content(HandoverEventFacts {
                target_id: "product-1".into(),
                target_number: Some("CP-001".into()),
                responsibility_changed: true,
                organization_changed: false,
            }))
            .unwrap();
        let event = log.structured_event.unwrap();
        assert_eq!(event.command_id.as_deref(), Some(command.id()));
        assert_eq!(event.facts[0].field_label, "交接结果");
        assert!(log.message.unwrap().contains("已交接"));
    }
}
