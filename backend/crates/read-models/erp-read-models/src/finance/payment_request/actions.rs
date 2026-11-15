//! 申请详情动作由当前目标权限和完整来源资格共同决定。

use application_core::AuditActor;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use erp_identity::{AccessTarget, Error as IdentityError};
use erp_permissions::{
    Permission as AccessPermission, purchase_payment_request as purchase_payment_request_permissions,
};
use persistence_core::Executor;

use super::PurchasePaymentRequestReadService;
use crate::Result;

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
            if self
                .actionable(actor, request, &purchase_payment_request_permissions::UPDATE, executor)
                .await?
            {
                actions.push("EDIT".into());
            }
            if self
                .actionable(actor, request, &purchase_payment_request_permissions::SUBMIT, executor)
                .await?
            {
                actions.push("SUBMIT".into());
            }
        }
        if request.status == PurchasePaymentRequestStatus::InApproval
            && self
                .actionable(actor, request, &purchase_payment_request_permissions::WITHDRAW, executor)
                .await?
        {
            actions.push("WITHDRAW".into());
        }
        if request.status.can_close()
            && self.actionable(actor, request, &purchase_payment_request_permissions::CLOSE, executor).await?
        {
            actions.push("CLOSE".into());
        }
        Ok(actions)
    }

    async fn actionable(
        &self,
        actor: &AuditActor,
        request: &PurchasePaymentRequest,
        permission: &AccessPermission,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        match self.rbac.require(actor, permission, AccessTarget::record(&request.base.id)?, executor).await {
            Ok(()) => Ok(true),
            Err(IdentityError::Forbidden(_) | IdentityError::NotFound(_)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}
