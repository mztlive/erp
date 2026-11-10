//! 审批抄送：配置资格、事件收件和独立只读授权。
mod read;
mod runtime;
use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use entity_core::BaseModel;
use mongodb::Database;
use persistence_core::Executor;
pub use read::{CopyDetail, CopyHistoryPage, CopyReadRequest};
pub use runtime::{persist_copies, persist_started_copy};

use crate::entity::approval_copy::{ApprovalCopyConfig, ApprovalCopyRule};
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::approval_copy::ApprovalCopyExt;

/// 重验当前账号为有效内部账号；抄送读取不依赖角色权限。
/// # 参数
/// 认证主体、授权端口、原执行器。
/// # 返回
/// 有效内部账号时成功，具体资料另按本人收件记录授权。
/// # 错误
/// 账号停用、身份不匹配或基础设施失败时拒绝。
pub async fn require_reader(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    ex: &mut dyn Executor,
) -> Result<()> {
    let account = auth.load_account(actor.id(), ex).await?;
    if !account.is_some_and(|a| a.kind == actor.kind() && a.is_active_backoffice()) {
        return Err(Error::Forbidden("当前账号不可读取抄送".into()));
    }
    Ok(())
}

/// 发布及保存时验证所有接收人均为有效内部账号。
/// # 参数
/// 授权端口、规则、原执行器。
/// # 返回
/// 全员有效时成功。
/// # 错误
/// 账号不存在、已停用或不是内部账号时返回校验错误；基础设施错误透传。
pub async fn validate_recipients(
    auth: &impl WorkflowAuthorizationPort,
    rules: &[ApprovalCopyRule],
    ex: &mut dyn Executor,
) -> Result<()> {
    let ids = rules
        .iter()
        .flat_map(|r| r.recipient_user_ids.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    let accounts = auth.load_accounts(&ids.iter().cloned().collect::<Vec<_>>(), ex).await?;
    for id in ids {
        accounts
            .iter()
            .find(|a| a.id == id && a.is_active_backoffice())
            .ok_or_else(|| Error::ValidationError("抄送人不存在、已停用或不是内部账号，请重新选择".into()))?;
    }
    Ok(())
}

/// 在草稿图的 CAS 事务中保存完整抄送配置。
/// # 参数
/// 数据库、规则、定义图和事务。
/// # 返回
/// 无。
/// # 错误
/// 校验或数据库错误。
pub async fn save_config(
    db: &Database,
    graph: &DefinitionGraph,
    auth: &impl WorkflowAuthorizationPort,
    rules: Vec<ApprovalCopyRule>,
    ex: &mut dyn Executor,
) -> Result<()> {
    let rules = ApprovalCopyRule::prepare(rules, graph.nodes.len())?;
    let ids = rules.iter().flat_map(|r| r.recipient_user_ids.iter().cloned()).collect::<Vec<_>>();
    let recipient_names =
        auth.load_accounts(&ids, ex).await?.into_iter().map(|a| (a.id, a.display_name)).collect();
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
    use std::future::Future;

    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::super::definition_assignees::eligible_candidate;
    use super::*;
    use crate::entity::approval_copy::CopyTrigger;
    use crate::ports::WorkflowAccountFact;

    #[derive(Clone)]
    struct AccountsOnly(Vec<WorkflowAccountFact>);

    #[allow(clippy::manual_async_fn)]
    impl WorkflowAuthorizationPort for AccountsOnly {
        fn load_account(
            &self,
            _account_id: &str,
            _executor: &mut dyn Executor,
        ) -> impl Future<Output = Result<Option<WorkflowAccountFact>>> + Send {
            async move { Ok(self.0.iter().find(|a| a.id == _account_id).cloned()) }
        }

        fn load_accounts(
            &self,
            _account_ids: &[String],
            _executor: &mut dyn Executor,
        ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
            async move { Ok(self.0.iter().filter(|a| _account_ids.contains(&a.id)).cloned().collect()) }
        }

        fn list_accounts_by_kind(
            &self,
            _account_kind: AccountKind,
            _executor: &mut dyn Executor,
        ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
            async move { Err(Error::Internal("unexpected role permission check".into())) }
        }

        fn list_active_approval_candidates(
            &self,
            _search: Option<&str>,
            _limit: u32,
            _executor: &mut dyn Executor,
        ) -> impl Future<Output = Result<Vec<WorkflowAccountFact>>> + Send {
            async move { Err(Error::Internal("unexpected role permission check".into())) }
        }
    }

    fn account(id: &str, active: bool) -> WorkflowAccountFact {
        WorkflowAccountFact::new(id, AccountKind::Admin, active).with_login_account(id)
    }

    fn rule(id: &str) -> ApprovalCopyRule {
        ApprovalCopyRule {
            trigger: CopyTrigger::Started,
            node_order: None,
            recipient_user_ids: vec![id.into()],
        }
    }

    #[tokio::test]
    async fn internal_recipient_needs_no_role_or_approval_permission() {
        let member = account("member", true);
        let auth = AccountsOnly(vec![member.clone()]);
        assert!(eligible_candidate(&auth, &member, true).await.unwrap());
        validate_recipients(&auth, &[rule("member")], &mut NoTransaction).await.unwrap();
        let actor = AuditActor::new("member".into(), "member".into(), AccountKind::Admin);
        require_reader(&auth, &actor, &mut NoTransaction).await.unwrap();
        // 同一个人当审批人时仍须校验角色权限；替身拒绝所有角色读取。
        assert!(eligible_candidate(&auth, &member, false).await.is_err());
    }

    #[tokio::test]
    async fn missing_disabled_and_non_internal_recipients_are_rejected() {
        let disabled = account("disabled", false);
        let external = WorkflowAccountFact::new("external", AccountKind::Supplier, true);
        let auth = AccountsOnly(vec![disabled.clone(), external.clone()]);
        for member in [&disabled, &external] {
            assert!(!eligible_candidate(&auth, member, true).await.unwrap());
        }
        for id in ["missing", "disabled", "external"] {
            assert!(validate_recipients(&auth, &[rule(id)], &mut NoTransaction).await.is_err());
            let actor = AuditActor::new(id.into(), id.into(), AccountKind::Admin);
            assert!(matches!(
                require_reader(&auth, &actor, &mut NoTransaction).await,
                Err(Error::Forbidden(_))
            ));
        }
    }

    #[tokio::test]
    async fn reader_kind_mismatch_and_account_lookup_failure_are_rejected() {
        let auth = AccountsOnly(vec![account("member", true)]);
        let actor = AuditActor::new("member".into(), "member".into(), AccountKind::Supplier);
        assert!(require_reader(&auth, &actor, &mut NoTransaction).await.is_err());
        let auth = crate::ports::FailClosedWorkflowAuthorizationPort;
        assert!(require_reader(&auth, &actor, &mut NoTransaction).await.is_err());
        assert!(validate_recipients(&auth, &[rule("member")], &mut NoTransaction).await.is_err());
    }
}
