//! 交接命令的规范化、目标组织检查和安全中文事件事实。

use std::slice;
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{
    AuditAction, AuditCode, AuditFact, AuditField, AuditFieldKind, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult,
};
use erp_core::AccountKind;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, AccountPermissionCandidate, AuthorizationService, Permission};
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

/// 通过统一授权服务检查实际目标账号的功能资格。
/// # 参数
/// db、rbac 为身份设施，target 为目标账号，permission 为已登记的具体功能，executor 为原事务。
/// # 返回
/// 账号有效且功能资格成立时返回 true。
/// # 错误
/// 身份设施或策略读取失败时透传。
pub(crate) async fn target_qualified(
    _db: &Database,
    rbac: &Arc<AuthorizationService>,
    target: &str,
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<bool> {
    // 交接目标只允许内部账号；统一资格检查负责重读账号并核验真实类型和状态。
    let actor = AuditActor::new(target.to_owned(), target.to_owned(), AccountKind::Admin);
    let authorization = rbac.clone();
    Ok(authorization.has_permission(&actor, permission, executor).await?)
}

/// 通过统一功能资格取得交接目标展示候选。
/// # 参数
/// db、rbac为身份设施，excluded为当前责任人，permission为目标岗位功能，executor为当前读取阶段。
/// # 返回
/// 按姓名、账号、ID排序的候选。
/// # 错误
/// 账号、权限或身份设施读取失败时停止。
pub(crate) async fn qualified_candidates(
    db: &Database,
    rbac: &Arc<AuthorizationService>,
    excluded: &str,
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<Vec<AccountPermissionCandidate>> {
    let service = rbac.clone();
    let mut candidates = Vec::new();
    for account in db.accounts().list_by_kind(AccountKind::Admin, executor).await? {
        if account.base.id == excluded {
            continue;
        }
        let actor = AuditActor::new(account.base.id.clone(), account.name.clone(), account.kind);
        if let Some(qualified) = service.permissions(&actor, slice::from_ref(permission), executor).await?
            && qualified.permissions().covers_one(permission)
        {
            let current = qualified.account();
            candidates.push(AccountPermissionCandidate {
                user_id: current.id.clone(),
                display_name: current.display_name.clone(),
                account: current.login_account.clone(),
            });
        }
    }
    candidates.sort();
    Ok(candidates)
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
