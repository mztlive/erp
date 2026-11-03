//! 组合根注入的审批领域动作注册表。
//!
//! 本模块位于审批运行时与业务域之外。审批运行时只依赖
//! [`ApprovalDomainActionPort`]；本注册表负责把合同动作路由到所属业务域，
//! 并强制复用审批运行时持有的唯一事务执行器。

use std::sync::Arc;

use application_core::AuditActor;
use erp_finance_posting::finance_posting;
use erp_fulfillment_execution::inventory_adjustment;
use erp_identity::SharedRbacService;
use erp_order_to_cash::{order_to_cash, sales_change};
use erp_procure_to_pay::procure_to_pay;
use erp_reverse_flow::reverse_flow;
use erp_workflow::service::approval::policy::ApprovalDomainAction;
use erp_workflow::{ApprovalActionContext, ApprovalActionFuture, ApprovalDomainActionPort};
use mongodb::Database;
use persistence_core::Executor;

use super::PaymentRequestRestoration;
use super::error::{DispatchError as ServiceError, Result as ServiceResult, map_service_error};
use crate::wiring::sales::sales_services;
use crate::wiring::{
    finance_posting_ports, inventory_adjustment_process, purchase_formalization_process, purchase_ports,
    reverse_flow_ports,
};

/// 审批强类型领域动作注册表。
pub struct ApprovalActionRegistry {
    db: Database,
    rbac: SharedRbacService,
}

impl ApprovalActionRegistry {
    /// 构造完整领域动作注册表。
    ///
    /// # 参数
    /// * `db` - 各领域动作使用的数据库。
    /// * `rbac` - 领域动作重验使用的 RBAC 快照。
    ///
    /// # 返回
    /// 返回可注入审批运行时的注册表。
    ///
    /// # 错误
    /// 不返回错误。
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
        | ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission
        | ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange
        | ApprovalDomainAction::SalesOrderCancelApprovalSubmission
        | ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission
        | ApprovalDomainAction::SalesChangeOrderCancelApproval => {
            dispatch_sales(registry, action, context, actor, executor).await
        },
        ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder
        | ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange
        | ApprovalDomainAction::PurchaseOrderCancelApproval
        | ApprovalDomainAction::PurchaseChangeOrderCancelApproval => {
            dispatch_procurement(registry, action, context, actor, executor).await
        },
        ApprovalDomainAction::StockAdjustmentPost | ApprovalDomainAction::StockAdjustmentCancelApproval => {
            dispatch_inventory(registry, action, context, actor, executor).await
        },
        ApprovalDomainAction::SalesInvoiceRequestApprove
        | ApprovalDomainAction::SalesInvoiceRequestCancelApproval
        | ApprovalDomainAction::CustomerReceiptPost
        | ApprovalDomainAction::CustomerReceiptCancelApproval => {
            dispatch_finance(registry, action, context, actor, executor).await
        },
        ApprovalDomainAction::CustomerRefundPost
        | ApprovalDomainAction::SupplierRefundPost
        | ApprovalDomainAction::ReceiptReversalPost
        | ApprovalDomainAction::PaymentReversalPost
        | ApprovalDomainAction::CustomerRefundCancelApproval
        | ApprovalDomainAction::SupplierRefundCancelApproval
        | ApprovalDomainAction::ReceiptReversalCancelApproval
        | ApprovalDomainAction::PaymentReversalCancelApproval => {
            dispatch_reverse(registry, action, context, actor, executor).await
        },
        ApprovalDomainAction::PurchasePaymentRequestApprove
        | ApprovalDomainAction::PurchasePaymentRequestReject
        | ApprovalDomainAction::PurchasePaymentRequestCancelApproval => {
            dispatch_payment_request(registry, action, context, actor, executor).await
        },
        _ => rejected_action(action),
    }
}

async fn dispatch_sales(
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
            order_to_cash::SalesOrderFormalizationProcess::new(
                registry.db.clone(),
                registry.rbac.clone(),
                sales_services(registry.db.clone(), registry.rbac.clone()),
            )
            .formalize_approved_submission_apply(context.business_object_id(), actor, executor)
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange => {
            require_transaction(executor)?;
            sales_change::SalesChangeProcess::new(
                registry.db.clone(),
                registry.rbac.clone(),
                sales_services(registry.db.clone(), registry.rbac.clone()),
            )
            .apply_effective_change_apply(context.business_object_id(), actor, executor)
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::SalesOrderCancelApprovalSubmission
        | ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission => order_to_cash::cancel_approval(
            &registry.db,
            context.business_object_id(),
            action,
            actor,
            executor,
        )
        .await
        .map_err(Into::into),
        ApprovalDomainAction::SalesChangeOrderCancelApproval => {
            sales_change::cancel_approval(&registry.db, context.business_object_id(), action, actor, executor)
                .await
                .map_err(Into::into)
        },
        _ => rejected_action(action),
    }
}

async fn dispatch_procurement(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    match action {
        ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder => {
            require_transaction(executor)?;
            purchase_formalization_process(registry.db.clone(), registry.rbac.clone())
                .formalize_approved_order_apply(context.business_object_id(), actor, executor)
                .await
                .map_err(Into::into)
        },
        ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange => {
            require_transaction(executor)?;
            procure_to_pay::PurchaseOrderProcess::new(
                registry.db.clone(),
                purchase_ports(registry.db.clone(), registry.rbac.clone()),
            )
            .apply_effective_change_apply(context.business_object_id(), actor, executor)
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::PurchaseOrderCancelApproval => procure_to_pay::cancel_order_approval(
            &registry.db,
            context.business_object_id(),
            action,
            actor,
            executor,
        )
        .await
        .map_err(Into::into),
        ApprovalDomainAction::PurchaseChangeOrderCancelApproval => {
            procure_to_pay::cancel_change_approval_apply(
                &registry.db,
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        _ => rejected_action(action),
    }
}

async fn dispatch_inventory(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    match action {
        ApprovalDomainAction::StockAdjustmentPost => {
            inventory_adjustment_process(registry.db.clone(), registry.rbac.clone())
                .post_stock_adjustment(context, actor, executor)
                .await
                .map(|_| ())
                .map_err(Into::into)
        },
        ApprovalDomainAction::StockAdjustmentCancelApproval => {
            inventory_adjustment::cancel_approval::cancel_stock_adjustment_approval_apply(
                &registry.db,
                context,
                action,
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        _ => rejected_action(action),
    }
}

async fn dispatch_finance(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    match action {
        ApprovalDomainAction::SalesInvoiceRequestApprove => {
            finance_posting::receivable::invoice_request::approve(
                &registry.db,
                &finance_posting_ports(registry.db.clone(), registry.rbac.clone()),
                context.business_object_id(),
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::SalesInvoiceRequestCancelApproval => {
            finance_posting::receivable::invoice_request::cancel(
                &registry.db,
                context.business_object_id(),
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::CustomerReceiptPost => {
            require_transaction(executor)?;
            finance_posting::receivable::post_customer_receipt_apply(
                &registry.db,
                context.business_object_id(),
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::CustomerReceiptCancelApproval => {
            finance_posting::receivable::cancel_customer_receipt_approval_apply(
                &registry.db,
                context.business_object_id(),
                action,
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        _ => rejected_action(action),
    }
}

async fn dispatch_reverse(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    match action {
        ApprovalDomainAction::CustomerRefundPost | ApprovalDomainAction::SupplierRefundPost => {
            require_transaction(executor)?;
            reverse_flow::finalize_approved_return(
                &registry.db,
                &reverse_flow_ports(registry.db.clone(), registry.rbac.clone()),
                action.document_type(),
                context.business_object_id(),
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::ReceiptReversalPost => {
            require_transaction(executor)?;
            reverse_flow::ReceiptReversalProcess::new(registry.db.clone())
                .post_receipt_reversal_apply(context.business_object_id(), actor, executor)
                .await
                .map_err(Into::into)
        },
        ApprovalDomainAction::PaymentReversalPost => {
            require_transaction(executor)?;
            reverse_flow::PaymentReversalProcess::new(
                registry.db.clone(),
                registry.rbac.clone(),
                Arc::new(PaymentRequestRestoration::new(registry.db.clone(), registry.rbac.clone())),
            )
            .post_payment_reversal_apply(context.business_object_id(), actor, executor)
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::CustomerRefundCancelApproval
        | ApprovalDomainAction::SupplierRefundCancelApproval
        | ApprovalDomainAction::ReceiptReversalCancelApproval
        | ApprovalDomainAction::PaymentReversalCancelApproval => reverse_flow::cancel_approval(
            &registry.db,
            action.document_type(),
            context.business_object_id(),
            action,
            actor,
            executor,
        )
        .await
        .map_err(Into::into),
        _ => rejected_action(action),
    }
}

async fn dispatch_payment_request(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    use finance_posting::payable::payment_request;
    require_transaction(executor)?;
    payment_request::validate_approval_context(&registry.db, context, executor).await?;
    let id = context.business_object_id();
    let reason = context.reason().unwrap_or("审批取消，详细信息见审批记录");
    match action {
        ApprovalDomainAction::PurchasePaymentRequestApprove => payment_request::approve(
            &registry.db,
            &finance_posting_ports(registry.db.clone(), registry.rbac.clone()),
            id,
            actor,
            executor,
        )
        .await
        .map_err(Into::into),
        ApprovalDomainAction::PurchasePaymentRequestReject => {
            payment_request::reject(&registry.db, id, actor, reason, executor).await.map_err(Into::into)
        },
        ApprovalDomainAction::PurchasePaymentRequestCancelApproval => {
            payment_request::cancel_approval(&registry.db, id, actor, reason, executor)
                .await
                .map_err(Into::into)
        },
        _ => rejected_action(action),
    }
}

fn rejected_action(action: ApprovalDomainAction) -> ServiceResult<()> {
    Err(ServiceError::BusinessLogicError(format!(
        "动作 {} 必须由业务域提交入口执行，审批运行时不得反向调用",
        action.as_str()
    )))
}

/// 审批领域动作必须复用调用方事务；没有会话时拒绝，避免在事务外生效。
fn require_transaction(executor: &mut dyn Executor) -> ServiceResult<()> {
    if executor.session().is_none() {
        return Err(ServiceError::Internal("审批领域动作缺少事务执行器".to_string()));
    }
    Ok(())
}

/// 冻结单据类型必须与动作一致，操作人必须与认证身份一致。
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_workflow::DocumentType;
    use erp_workflow::service::approval::action::DecisionActionParams;
    use persistence_core::NoTransaction;

    use super::*;

    fn actor(id: &str) -> AuditActor {
        AuditActor::new(id.into(), "operator".into(), AccountKind::Admin)
    }

    fn context(document_type: DocumentType, actor_id: &str) -> ApprovalActionContext {
        ApprovalActionContext::for_decision(DecisionActionParams {
            approval_process_instance_id: "instance-1".into(),
            approval_node_execution_id: "execution-1".into(),
            work_item_id: "task-1".into(),
            business_object_type: document_type.as_str().into(),
            business_object_id: "document-1".into(),
            subject_version: "3".into(),
            actor_id: actor_id.into(),
            reason: None,
            idempotency_key: "decision-1".into(),
        })
        .unwrap()
    }

    #[test]
    fn valid_context_keeps_exact_action_document_and_authenticated_actor() {
        for action in ApprovalDomainAction::ALL {
            let context = context(action.document_type(), "actor-1");
            validate_context(action, &context, &actor("actor-1")).unwrap();
            assert_eq!(context.business_object_id(), "document-1");
            assert_eq!(context.subject_version(), "3");
        }
    }

    #[test]
    fn wrong_document_stops_before_actor_and_correct_document_rejects_another_actor() {
        let action = ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder;
        let wrong = context(DocumentType::SalesOrder, "another-actor");
        assert!(
            matches!(validate_context(action, &wrong, &actor("actor-1")), Err(ServiceError::ConflictError(message)) if message == "审批领域动作与冻结单据类型不一致")
        );
        let original = context(DocumentType::PurchaseOrder, "another-actor");
        assert!(
            matches!(validate_context(action, &original, &actor("actor-1")), Err(ServiceError::Forbidden(message)) if message == "审批领域动作操作人与认证身份不一致")
        );
    }

    #[test]
    fn approval_write_requires_the_callers_transaction_executor() {
        assert!(
            matches!(require_transaction(&mut NoTransaction), Err(ServiceError::Internal(message)) if message == "审批领域动作缺少事务执行器")
        );
    }

    #[test]
    fn submission_actions_stay_outside_runtime_reverse_dispatch() {
        let action = ApprovalDomainAction::PurchaseOrderSubmit;
        assert!(
            matches!(rejected_action(action), Err(ServiceError::BusinessLogicError(message)) if message.contains(action.as_str()))
        );
    }
}
