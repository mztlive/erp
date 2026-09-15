//! 退款启动回放的真实授权提供方；主体、动作与对象范围按原顺序读取。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::SharedRbacService;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::{
    ApprovalManagementScope, approval_actor_is_active_with_executor,
    approval_document_action_scope_with_executor, approval_document_read_scope_with_executor,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

#[async_trait]
trait ReplayAuthorizationPort: Send + Sync {
    async fn actor_active(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<bool>;
    async fn action_scope(
        &self,
        actor: &AuditActor,
        permission: &str,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalManagementScope>;
    async fn read_scope(
        &self,
        actor: &AuditActor,
        document_type: DocumentType,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalManagementScope>;
}
struct MongoReplayAuthorization<'a> {
    db: &'a Database,
    rbac: &'a SharedRbacService,
}
#[async_trait]
impl ReplayAuthorizationPort for MongoReplayAuthorization<'_> {
    async fn actor_active(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<bool> {
        Ok(approval_actor_is_active_with_executor(
            &crate::adapters::workflow::workflow_auth(self.db.clone(), self.rbac.clone()),
            actor,
            executor,
        )
        .await?)
    }
    async fn action_scope(
        &self,
        actor: &AuditActor,
        permission: &str,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalManagementScope> {
        Ok(approval_document_action_scope_with_executor(
            &crate::adapters::workflow::workflow_auth(self.db.clone(), self.rbac.clone()),
            actor,
            permission,
            executor,
        )
        .await?)
    }
    async fn read_scope(
        &self,
        actor: &AuditActor,
        document_type: DocumentType,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalManagementScope> {
        Ok(approval_document_read_scope_with_executor(
            &crate::adapters::workflow::workflow_auth(self.db.clone(), self.rbac.clone()),
            actor,
            document_type,
            executor,
        )
        .await?)
    }
}
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
async fn authorize<P: ReplayAuthorizationPort>(
    port: &P,
    actor: &AuditActor,
    document_type: DocumentType,
    permission: &str,
    organization_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_active(port, actor, executor).await?;
    let action_scope = port.action_scope(actor, permission, executor).await?;
    let read_scope = port.read_scope(actor, document_type, executor).await?;
    let object = erp_workflow::ports::WorkflowScopeObject {
        settlement_party_id: Some(organization_id.into()),
        ..Default::default()
    };
    if !action_scope.covers_object(&object) || !read_scope.covers_object(&object) {
        return Err(Error::Forbidden("无权提交该责任组织的退款或冲正单".to_string()));
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
    document_type: DocumentType,
    permission: &str,
    organization_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorize(
        &MongoReplayAuthorization { db, rbac },
        actor,
        document_type,
        permission,
        organization_id,
        executor,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    struct SessionMarker(u64);
    impl Executor for SessionMarker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.0 += 1;
            None
        }
    }
    struct RecordingPort {
        executor: usize,
        calls: Mutex<Vec<&'static str>>,
        active: bool,
        action: ApprovalManagementScope,
        read: ApprovalManagementScope,
        fail: Option<usize>,
    }
    impl RecordingPort {
        fn record(&self, step: &'static str, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(actor.id(), "actor-1");
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            let mut calls = self.calls.lock().unwrap();
            let index = calls.len();
            calls.push(step);
            if self.fail == Some(index) {
                return Err(Error::ConflictError(format!("authorization failure {index}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl ReplayAuthorizationPort for RecordingPort {
        async fn actor_active(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<bool> {
            self.record("actor", actor, executor)?;
            Ok(self.active)
        }
        async fn action_scope(
            &self,
            actor: &AuditActor,
            permission: &str,
            executor: &mut dyn Executor,
        ) -> Result<ApprovalManagementScope> {
            self.record("action", actor, executor)?;
            assert_eq!(permission, "customer_refund:submit");
            Ok(self.action.clone())
        }
        async fn read_scope(
            &self,
            actor: &AuditActor,
            document_type: DocumentType,
            executor: &mut dyn Executor,
        ) -> Result<ApprovalManagementScope> {
            self.record("read", actor, executor)?;
            assert_eq!(document_type, DocumentType::CustomerRefund);
            Ok(self.read.clone())
        }
    }
    async fn invoke(
        active: bool,
        action: ApprovalManagementScope,
        read: ApprovalManagementScope,
        fail: Option<usize>,
    ) -> (Result<()>, Vec<&'static str>) {
        let mut executor = SessionMarker(91);
        let port = RecordingPort {
            executor: &mut executor as *mut SessionMarker as usize,
            calls: Mutex::new(Vec::new()),
            active,
            action,
            read,
            fail,
        };
        let actor = AuditActor::new("actor-1".into(), "actor".into(), erp_core::AccountKind::Admin);
        let result = authorize(
            &port,
            &actor,
            DocumentType::CustomerRefund,
            "customer_refund:submit",
            "party-1",
            &mut executor,
        )
        .await;
        assert_eq!(executor.0, 91);
        (result, port.calls.into_inner().unwrap())
    }
    fn organization(id: &str) -> ApprovalManagementScope {
        struct Party(String);
        impl erp_workflow::ports::WorkflowScopePredicate for Party {
            fn allows(&self, object: &erp_workflow::ports::WorkflowScopeObject) -> bool {
                object.settlement_party_id.as_deref() == Some(self.0.as_str())
            }
        }
        ApprovalManagementScope::Resolved(erp_workflow::ports::WorkflowDataScope::new(
            "customer_refund".into(),
            "submit".into(),
            1,
            id.into(),
            vec!["finance".into()],
            true,
            std::sync::Arc::new(Party(id.into())),
        ))
    }
    #[tokio::test]
    async fn replay_authorization_uses_one_executor_and_original_actor_action_read_order() {
        let (result, calls) = invoke(true, organization("party-1"), organization("party-1"), None).await;
        result.unwrap();
        assert_eq!(calls, ["actor", "action", "read"]);
    }
    #[tokio::test]
    async fn replay_authorization_stops_on_each_original_provider_error() {
        for index in 0..3 {
            let (result, calls) =
                invoke(true, ApprovalManagementScope::Empty, ApprovalManagementScope::Empty, Some(index))
                    .await;
            assert!(
                matches!(result,Err(Error::ConflictError(message)) if message==format!("authorization failure {index}"))
            );
            assert_eq!(calls, ["actor", "action", "read"][..=index]);
        }
    }
    #[tokio::test]
    async fn disabled_actor_fails_before_action_or_object_scope() {
        let (result, calls) =
            invoke(false, ApprovalManagementScope::Empty, ApprovalManagementScope::Empty, None).await;
        assert!(matches!(result,Err(Error::Forbidden(message)) if message=="当前账号不可提交该退款或冲正单"));
        assert_eq!(calls, ["actor"]);
    }
    #[tokio::test]
    async fn either_scope_must_cover_the_organization_after_both_reads() {
        for (action, read) in [
            (organization("other"), organization("party-1")),
            (organization("party-1"), organization("other")),
        ] {
            let (result, calls) = invoke(true, action, read, None).await;
            assert!(
                matches!(result,Err(Error::Forbidden(message)) if message=="无权提交该责任组织的退款或冲正单")
            );
            assert_eq!(calls, ["actor", "action", "read"]);
        }
    }
}
