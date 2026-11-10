//! 审批抄送：配置资格、事件收件和独立只读授权。
mod read;
mod runtime;
use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use entity_core::BaseModel;
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::Executor;
pub use read::{CopyDetail, CopyHistoryPage, CopyReadRequest};
pub use runtime::{persist_copies, persist_started_copy};

use crate::entity::approval_copy::{ApprovalCopyConfig, ApprovalCopyRule};
use crate::error::{Error, Result};
use crate::repository::approval_copy::ApprovalCopyExt;

/// 重验当前账号为有效内部账号；抄送读取不依赖角色权限。
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
///
/// 认证主体、身份服务、原执行器。
/// # 返回
/// 有效内部账号时成功，具体资料另按本人收件记录授权。
/// # 错误
/// 账号停用、身份不匹配或基础设施失败时拒绝。
pub async fn require_reader(
    identity_service: &SharedRbacService,
    actor: &AuditActor,
    ex: &mut dyn Executor,
) -> Result<()> {
    let account = identity_service.directory().account(actor.id(), ex).await?;
    if !account.is_some_and(|a| a.kind == actor.kind() && a.is_active_backoffice()) {
        return Err(Error::Forbidden("当前账号不可读取抄送".into()));
    }
    Ok(())
}

/// 发布及保存时验证所有接收人均为有效内部账号。
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
///
/// 身份服务、规则、原执行器。
/// # 返回
/// 全员有效时成功。
/// # 错误
/// 账号不存在、已停用或不是内部账号时返回校验错误；基础设施错误透传。
pub async fn validate_recipients(
    identity_service: &SharedRbacService,
    rules: &[ApprovalCopyRule],
    ex: &mut dyn Executor,
) -> Result<()> {
    let ids = rules
        .iter()
        .flat_map(|r| r.recipient_user_ids.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    let accounts =
        identity_service.directory().accounts(&ids.iter().cloned().collect::<Vec<_>>(), ex).await?;
    for rule in rules {
        rule.validate_accounts(&accounts)?;
    }
    Ok(())
}

/// 在草稿图的 CAS 事务中保存完整抄送配置。
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
///
/// 数据库、规则、定义图和事务。
/// # 返回
/// 无。
/// # 错误
/// 校验或数据库错误。
pub async fn save_config(
    identity_service: &SharedRbacService,
    db: &Database,
    graph: &DefinitionGraph,
    rules: Vec<ApprovalCopyRule>,
    ex: &mut dyn Executor,
) -> Result<()> {
    let rules = ApprovalCopyRule::prepare(rules, graph.nodes.len())?;
    let ids = rules.iter().flat_map(|r| r.recipient_user_ids.iter().cloned()).collect::<Vec<_>>();
    let recipient_names = identity_service
        .directory()
        .accounts(&ids, ex)
        .await?
        .into_iter()
        .map(|a| (a.id, a.display_name))
        .collect();
    db.approval_copies()
        .save_config(
            &ApprovalCopyConfig {
                base: BaseModel::new(graph.definition.base.id.clone()),
                rules,
                recipient_names,
            },
            ex,
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::AccountSummary;

    use super::*;
    use crate::entity::approval_copy::CopyTrigger;

    fn rule(id: &str) -> ApprovalCopyRule {
        ApprovalCopyRule {
            trigger: CopyTrigger::Started,
            node_order: None,
            recipient_user_ids: vec![id.into()],
        }
    }

    #[test]
    fn internal_recipient_needs_no_role_or_approval_permission() {
        let member = AccountSummary::new("member", AccountKind::Admin, true);
        rule("member").validate_accounts(&[member]).unwrap();
    }

    #[test]
    fn missing_disabled_and_non_internal_recipients_are_rejected() {
        let accounts = [
            AccountSummary::new("disabled", AccountKind::Admin, false),
            AccountSummary::new("external", AccountKind::Supplier, true),
        ];
        for id in ["missing", "disabled", "external"] {
            assert!(rule(id).validate_accounts(&accounts).is_err());
        }
    }
}
