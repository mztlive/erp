//! 申请详情动作由当前目标权限和完整来源资格共同决定。

use application_core::AuditActor;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use persistence_core::Executor;

use super::PurchasePaymentRequestReadService;
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
        if editable {
            if self.actionable(actor, request, "update", executor).await? {
                actions.push("EDIT".into());
            }
            if self.actionable(actor, request, "submit", executor).await? {
                actions.push("SUBMIT".into());
            }
        }
        if request.status == PurchasePaymentRequestStatus::InApproval
            && self.actionable(actor, request, "withdraw", executor).await?
        {
            actions.push("WITHDRAW".into());
        }
        if request.status.can_close() && self.actionable(actor, request, "close", executor).await? {
            actions.push("CLOSE".into());
        }
        Ok(actions)
    }

    async fn actionable(
        &self,
        actor: &AuditActor,
        request: &PurchasePaymentRequest,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let scope = match self.source_scope(actor, action, executor).await {
            Ok(scope) => scope,
            Err(Error::Forbidden(_)) => return Ok(false),
            Err(error) => return Err(error),
        };
        self.readable(request, &scope, executor).await
    }
}
