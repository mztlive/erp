//! 组合根注入的审批领域动作注册表。
//!
//! 本模块位于审批运行时与业务域之外。审批运行时只依赖
//! [`ApprovalDomainActionPort`]；本注册表负责把合同动作路由到所属业务域，
//! 并强制复用审批运行时持有的唯一事务执行器。

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_workflow::service::approval::policy::ApprovalDomainAction;
use erp_workflow::{ApprovalActionContext, ApprovalActionFuture, ApprovalDomainActionPort};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error as ServiceError, Result as ServiceResult};

/// 审批强类型领域动作注册表。
pub struct ApprovalActionRegistry {
    db: Database,
    rbac: SharedRbacService,
}

impl ApprovalActionRegistry {
    /// 构造完整领域动作注册表。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
}

impl ApprovalDomainActionPort for ApprovalActionRegistry {
    fn execute<'a>(
        &'a self,
        action: ApprovalDomainAction,
        context: &'a ApprovalActionContext,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a> {
        Box::pin(async move {
            validate_context(action, context, actor).map_err(map_service_error)?;
            dispatch_action(self, action, context, actor, executor).await.map_err(map_service_error)
        })
    }
}

async fn dispatch_action(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    match action {
        ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission
        | ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission => {
            require_transaction(executor)?;
            crate::order_to_cash::SalesOrderFormalizationProcess::new(
                registry.db.clone(),
                registry.rbac.clone(),
            )
            .formalize_approved_submission_apply(context.business_object_id(), actor, executor)
            .await
        },
        ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange => {
            require_transaction(executor)?;
            crate::sales_change::SalesChangeProcess::new(registry.db.clone(), registry.rbac.clone())
                .apply_effective_change_apply(context.business_object_id(), actor, executor)
                .await
        },
        ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder => {
            require_transaction(executor)?;
            crate::procure_to_pay::PurchaseOrderFormalizationProcess::new(
                registry.db.clone(),
                registry.rbac.clone(),
            )
            .formalize_approved_order_apply(context.business_object_id(), actor, executor)
            .await
        },
        ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange => {
            require_transaction(executor)?;
            crate::procure_to_pay::PurchaseOrderProcess::new(registry.db.clone())
                .apply_effective_change_apply(context.business_object_id(), actor, executor)
                .await
        },
        ApprovalDomainAction::StockAdjustmentPost => {
            crate::inventory_adjustment::InventoryAdjustmentService::new(
                registry.db.clone(),
                registry.rbac.clone(),
            )
            .post_stock_adjustment(context, actor, executor)
            .await
            .map(|_| ())
        },
        ApprovalDomainAction::SalesInvoiceRequestApprove => {
            crate::finance_posting::receivable::invoice_request::approve(
                &registry.db,
                context.business_object_id(),
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::SalesInvoiceRequestCancelApproval => {
            crate::finance_posting::receivable::invoice_request::cancel(
                &registry.db,
                context.business_object_id(),
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::CustomerReceiptPost => {
            require_transaction(executor)?;
            crate::finance_posting::receivable::post_customer_receipt_apply(
                &registry.db,
                context.business_object_id(),
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::CustomerRefundPost | ApprovalDomainAction::SupplierRefundPost => {
            require_transaction(executor)?;
            crate::reverse_flow::finalize_approved_return(
                &registry.db,
                action.document_type(),
                context.business_object_id(),
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::ReceiptReversalPost => {
            require_transaction(executor)?;
            crate::reverse_flow::ReceiptReversalProcess::new(registry.db.clone())
                .post_receipt_reversal_apply(context.business_object_id(), actor, executor)
                .await
        },
        ApprovalDomainAction::PaymentReversalPost => {
            require_transaction(executor)?;
            crate::reverse_flow::PaymentReversalProcess::new(registry.db.clone(), registry.rbac.clone())
                .post_payment_reversal_apply(context.business_object_id(), actor, executor)
                .await
        },
        ApprovalDomainAction::SalesOrderCancelApprovalSubmission
        | ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission => {
            crate::order_to_cash::cancel_approval(
                &registry.db,
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::SalesChangeOrderCancelApproval => {
            crate::sales_change::cancel_approval(
                &registry.db,
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::PurchaseOrderCancelApproval => {
            crate::procure_to_pay::cancel_order_approval(
                &registry.db,
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::PurchaseChangeOrderCancelApproval => {
            crate::procure_to_pay::cancel_change_approval_apply(
                &registry.db,
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::StockAdjustmentCancelApproval => {
            crate::inventory_adjustment::cancel_approval::cancel_stock_adjustment_approval_apply(
                &registry.db,
                context,
                action,
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::CustomerReceiptCancelApproval => {
            crate::finance_posting::receivable::cancel_customer_receipt_approval_apply(
                &registry.db,
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
        },
        ApprovalDomainAction::CustomerRefundCancelApproval
        | ApprovalDomainAction::SupplierRefundCancelApproval
        | ApprovalDomainAction::ReceiptReversalCancelApproval
        | ApprovalDomainAction::PaymentReversalCancelApproval => {
            crate::reverse_flow::cancel_approval(
                &registry.db,
                action.document_type(),
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
        },
        _ => Err(ServiceError::BusinessLogicError(format!(
            "动作 {} 必须由业务域提交入口执行，审批运行时不得反向调用",
            action.as_str()
        ))),
    }
}

fn require_transaction(executor: &mut dyn Executor) -> ServiceResult<()> {
    if executor.session().is_none() {
        return Err(ServiceError::Internal("审批领域动作缺少事务执行器".to_string()));
    }
    Ok(())
}

fn validate_context(
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
) -> ServiceResult<()> {
    let expected = action.document_type();
    if context.business_object_type() != expected.as_str() {
        return Err(ServiceError::ConflictError("审批领域动作与冻结单据类型不一致".to_string()));
    }
    if context.actor_id() != actor.id() {
        return Err(ServiceError::Forbidden("审批领域动作操作人与认证身份不一致".to_string()));
    }
    Ok(())
}

fn map_service_error(error: ServiceError) -> erp_workflow::Error {
    match error {
        ServiceError::ValidationError(message) => erp_workflow::Error::ValidationError(message),
        ServiceError::NotFound(message) => erp_workflow::Error::NotFound(message),
        ServiceError::BusinessLogicError(message) => erp_workflow::Error::BusinessLogicError(message),
        ServiceError::ConflictError(message) => erp_workflow::Error::ConflictError(message),
        ServiceError::Forbidden(message) => erp_workflow::Error::Forbidden(message),
        ServiceError::Unauthenticated(message) => erp_workflow::Error::Unauthenticated(message),
        ServiceError::Internal(message) => erp_workflow::Error::Internal(message),
        ServiceError::Logic(error) => erp_workflow::Error::Logic(error),
        ServiceError::OutcomeUnknown(error) => erp_workflow::Error::OutcomeUnknown(error),
        ServiceError::RepositoryError(error) => erp_workflow::Error::RepositoryError(error),
        other => erp_workflow::Error::Internal(other.to_string()),
    }
}
