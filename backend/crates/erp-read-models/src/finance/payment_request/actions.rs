//! 申请详情动作由当前目标权限和完整来源资格共同决定。

use application_core::AuditActor;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use erp_identity::{AccessControlExt, Permission, RoleRepositoryExt};
use persistence_core::Executor;

use super::PurchasePaymentRequestReadService;
use crate::approval_runtime::has_document_permission;
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    pub(super) async fn allowed_actions(
        &self,
        actor: &AuditActor,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut actions = Vec::new();
        let editable = matches!(
            request.status,
            PurchasePaymentRequestStatus::Draft
                | PurchasePaymentRequestStatus::Rejected
                | PurchasePaymentRequestStatus::Withdrawn
        );
        if actor.id() == request.created_by && editable {
            if self.actionable(actor, request, "update", executor).await? {
                actions.push("EDIT".into());
            }
            if self.actionable(actor, request, "submit", executor).await? {
                actions.push("SUBMIT".into());
            }
        }
        if actor.id() == request.created_by
            && request.status == PurchasePaymentRequestStatus::InApproval
            && self.actionable(actor, request, "withdraw", executor).await?
        {
            actions.push("WITHDRAW".into());
        }
        if request.status.can_close()
            && self.actionable(actor, request, "close", executor).await?
            && self.creator_or_finance(actor, request, executor).await?
        {
            actions.push("CLOSE".into());
        }
        Ok(actions)
    }

    async fn creator_or_finance(
        &self,
        actor: &AuditActor,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        if request.created_by == actor.id() {
            return Ok(true);
        }
        let required = [
            Permission::parse("purchase_payment_request:close")?,
            Permission::parse("approval_instance:decide")?,
        ];
        let snapshot = self.rbac.role_permission_snapshot(actor.kind(), actor.id(), &required).await?;
        let roles = snapshot.granting_role_ids_for_all(&required);
        let allowed = !self.db.roles().enabled_roles(&roles, executor).await?.is_empty();
        self.rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        Ok(allowed)
    }

    async fn actionable(
        &self,
        actor: &AuditActor,
        request: &PurchasePaymentRequest,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        if !has_document_permission(&self.db, &self.rbac, actor, "purchase_payment_request", action, executor)
            .await?
        {
            return Ok(false);
        }
        let scope = match self.source_scope(actor, action, executor).await {
            Ok(scope) => scope,
            Err(Error::Forbidden(_)) => return Ok(false),
            Err(error) => return Err(error),
        };
        self.readable(request, &scope, executor).await
    }
}
