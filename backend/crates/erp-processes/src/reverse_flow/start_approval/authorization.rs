//! 退款提交和回放使用真实资金来源授权，不依赖尚未生成的审批任务。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::SharedRbacService;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::service::approval::{
    approval_action_roles_with_executor, approval_actor_is_active_with_executor,
};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::ReturnsProcess;
use crate::adapters::workflow::workflow_auth;
use crate::{Error, Result};

#[async_trait]
trait ReplayAuthorizationPort: Send + Sync {
    async fn actor_active(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<bool>;
    async fn action_allowed(
        &self,
        actor: &AuditActor,
        permission: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    async fn source_readable(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}
struct MongoReplayAuthorization<'a> {
    db: &'a Database,
    rbac: &'a SharedRbacService,
}
#[async_trait]
impl ReplayAuthorizationPort for MongoReplayAuthorization<'_> {
    async fn actor_active(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<bool> {
        Ok(approval_actor_is_active_with_executor(
            &workflow_auth(self.db.clone(), self.rbac.clone()),
            actor,
            executor,
        )
        .await?)
    }
    async fn action_allowed(
        &self,
        actor: &AuditActor,
        permission: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(!approval_action_roles_with_executor(
            &workflow_auth(self.db.clone(), self.rbac.clone()),
            actor,
            permission,
            executor,
        )
        .await?
        .is_empty())
    }
    async fn source_readable(
        &self,
        actor: &AuditActor,
        kind: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(workflow_auth(self.db.clone(), self.rbac.clone())
            .approval_source_readable(actor, kind, id, executor)
            .await?)
    }
}
/// 失效账号必须先于具体来源读取被拒绝。
async fn ensure_active<P: ReplayAuthorizationPort>(
    port: &P,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !port.actor_active(actor, executor).await? {
        return Err(Error::Forbidden("当前账号不可提交该退款或冲正单".to_string()));
    }
    Ok(())
}
/// 同一事务重验静态提交能力和完整资金来源。
async fn authorize<P: ReplayAuthorizationPort>(
    port: &P,
    actor: &AuditActor,
    kind: DocumentType,
    permission: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_active(port, actor, executor).await?;
    if !port.action_allowed(actor, permission, executor).await? {
        return Err(Error::Forbidden("当前账号缺少退款提交权限".to_string()));
    }
    if !port.source_readable(actor, kind, id, executor).await? {
        return Err(Error::Forbidden("无权读取退款所引用的完整资金来源".to_string()));
    }
    Ok(())
}
pub(super) async fn ensure_actor_active(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_active(&MongoReplayAuthorization { db, rbac }, actor, executor).await
}
pub(super) async fn ensure_replay_authorized(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    kind: DocumentType,
    permission: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorize(&MongoReplayAuthorization { db, rbac }, actor, kind, permission, id, executor).await?;
    match kind {
        DocumentType::CustomerRefund => ReturnsService::new(db.clone())
            .load_customer_refund(id, executor)
            .await?
            .ensure_submitter(actor.id())?,
        DocumentType::SupplierRefund => ReturnsService::new(db.clone())
            .load_supplier_refund(id, executor)
            .await?
            .ensure_submitter(actor.id())?,
        _ => {},
    }
    Ok(())
}

impl ReturnsProcess {
    /// 已提交命令回放仍在新事务内重验当前职责和资金来源，保持零写入。
    pub(in crate::reverse_flow) async fn authorize_refund_replay(
        &self,
        kind: DocumentType,
        permission: &str,
        id: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let permission = permission.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    ensure_replay_authorized(&db, &rbac, &actor, kind, &permission, &id, executor).await
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::NoTransaction;

    use super::*;

    struct RecordingPort {
        active: bool,
        action: bool,
        source: bool,
        calls: Mutex<Vec<&'static str>>,
    }
    #[async_trait]
    impl ReplayAuthorizationPort for RecordingPort {
        async fn actor_active(&self, _: &AuditActor, _: &mut dyn Executor) -> Result<bool> {
            self.calls.lock().unwrap().push("actor");
            Ok(self.active)
        }
        async fn action_allowed(
            &self,
            _: &AuditActor,
            permission: &str,
            _: &mut dyn Executor,
        ) -> Result<bool> {
            assert_eq!(permission, "customer_refund:submit");
            self.calls.lock().unwrap().push("action");
            Ok(self.action)
        }
        async fn source_readable(
            &self,
            _: &AuditActor,
            kind: DocumentType,
            id: &str,
            _: &mut dyn Executor,
        ) -> Result<bool> {
            assert_eq!((kind, id), (DocumentType::CustomerRefund, "refund-1"));
            self.calls.lock().unwrap().push("source");
            Ok(self.source)
        }
    }
    /// 首次提交与回放仅依赖自己的静态动作和精确资金来源，无审批参与权限依赖。
    #[tokio::test]
    async fn submission_checks_active_actor_action_and_exact_source() {
        for (active, action, source, expected_calls) in [
            (true, true, true, vec!["actor", "action", "source"]),
            (false, true, true, vec!["actor"]),
            (true, false, true, vec!["actor", "action"]),
            (true, true, false, vec!["actor", "action", "source"]),
        ] {
            let port = RecordingPort { active, action, source, calls: Mutex::new(Vec::new()) };
            let actor = AuditActor::new("actor-1".into(), "actor".into(), erp_core::AccountKind::Admin);
            let result = authorize(
                &port,
                &actor,
                DocumentType::CustomerRefund,
                "customer_refund:submit",
                "refund-1",
                &mut NoTransaction,
            )
            .await;
            assert_eq!(result.is_ok(), active && action && source);
            assert_eq!(port.calls.into_inner().unwrap(), expected_calls);
        }
    }
}
