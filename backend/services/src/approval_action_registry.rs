//! 组合根注入的审批领域动作注册表。
//!
//! 本模块位于审批运行时与业务域之外。审批运行时只依赖
//! [`ApprovalDomainActionPort`]；本注册表负责把合同动作路由到所属业务域，
//! 并强制复用审批运行时持有的唯一事务执行器。

use database::Executor;
use entities::document_registry::DocumentType;
use mongodb::Database;

use crate::approval::policy::ApprovalDomainAction;
use crate::approval::{ApprovalActionContext, ApprovalActionFuture, ApprovalDomainActionPort};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

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
            validate_context(action, context, actor)?;
            match action {
                ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission
                | ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission => {
                    let session = require_transaction(executor)?;
                    crate::sales_order::SalesOrderService::with_rbac(self.db.clone(), self.rbac.clone())
                        .formalize_approved_submission_in_transaction(
                            &context.business_object_id,
                            actor,
                            session,
                        )
                        .await
                }
                ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange => {
                    let session = require_transaction(executor)?;
                    crate::sales_review::SalesReviewService::new(self.db.clone())
                        .apply_effective_change_in_transaction(&context.business_object_id, actor, session)
                        .await
                }
                ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder => {
                    let session = require_transaction(executor)?;
                    crate::purchase_order::PurchaseOrderService::new(self.db.clone())
                        .formalize_approved_order_in_transaction(&context.business_object_id, actor, session)
                        .await
                }
                ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange => {
                    let session = require_transaction(executor)?;
                    crate::purchase_order::PurchaseOrderService::new(self.db.clone())
                        .apply_effective_change_in_transaction(&context.business_object_id, actor, session)
                        .await
                }
                ApprovalDomainAction::StockAdjustmentPost => {
                    let session = require_transaction(executor)?;
                    crate::inventory::post_stock_adjustment_in_transaction(
                        &self.db,
                        &entities::ids::StockAdjustmentId::new(&context.business_object_id),
                        actor,
                        session,
                    )
                    .await
                    .map(|_| ())
                }
                ApprovalDomainAction::CustomerReceiptPost => {
                    let session = require_transaction(executor)?;
                    crate::receivable::post_customer_receipt_in_transaction(
                        &self.db,
                        &context.business_object_id,
                        actor,
                        session,
                    )
                    .await
                }
                ApprovalDomainAction::CustomerRefundPost
                | ApprovalDomainAction::SupplierRefundPost
                | ApprovalDomainAction::ReceiptReversalPost
                | ApprovalDomainAction::PaymentReversalPost => {
                    let session = require_transaction(executor)?;
                    crate::returns::finalize_approved_return_in_transaction(
                        &self.db,
                        document_type_for_action(action),
                        &context.business_object_id,
                        actor,
                        session,
                    )
                    .await
                }
                ApprovalDomainAction::SalesOrderCancelApprovalSubmission
                | ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission => {
                    crate::sales_order::cancel_approval_in_transaction(
                        &self.db,
                        &context.business_object_id,
                        action,
                        actor,
                        executor,
                    )
                    .await
                }
                ApprovalDomainAction::SalesChangeOrderCancelApproval => {
                    crate::sales_review::cancel_approval_in_transaction(
                        &self.db,
                        &context.business_object_id,
                        action,
                        actor,
                        executor,
                    )
                    .await
                }
                ApprovalDomainAction::PurchaseOrderCancelApproval => {
                    crate::purchase_order::cancel_order_approval_in_transaction(
                        &self.db,
                        &context.business_object_id,
                        action,
                        actor,
                        executor,
                    )
                    .await
                }
                ApprovalDomainAction::PurchaseChangeOrderCancelApproval => {
                    crate::purchase_order::cancel_change_approval_in_transaction(
                        &self.db,
                        &context.business_object_id,
                        action,
                        actor,
                        executor,
                    )
                    .await
                }
                ApprovalDomainAction::StockAdjustmentCancelApproval => {
                    crate::inventory::cancel_stock_adjustment_approval_in_transaction(
                        &self.db,
                        &entities::ids::StockAdjustmentId::new(&context.business_object_id),
                        action,
                        actor,
                        executor,
                    )
                    .await
                }
                ApprovalDomainAction::CustomerReceiptCancelApproval => {
                    crate::receivable::cancel_customer_receipt_approval_in_transaction(
                        &self.db,
                        &context.business_object_id,
                        action,
                        actor,
                        executor,
                    )
                    .await
                }
                ApprovalDomainAction::CustomerRefundCancelApproval
                | ApprovalDomainAction::SupplierRefundCancelApproval
                | ApprovalDomainAction::ReceiptReversalCancelApproval
                | ApprovalDomainAction::PaymentReversalCancelApproval => {
                    crate::returns::cancel_approval_in_transaction(
                        &self.db,
                        document_type_for_action(action),
                        &context.business_object_id,
                        action,
                        actor,
                        executor,
                    )
                    .await
                }
                _ => Err(Error::BusinessLogicError(format!(
                    "动作 {} 必须由业务域提交入口执行，审批运行时不得反向调用",
                    action.as_str()
                ))),
            }
        })
    }
}

fn require_transaction(executor: &mut dyn Executor) -> Result<&mut mongodb::ClientSession> {
    executor
        .session()
        .ok_or_else(|| Error::Internal("审批领域动作缺少事务会话".to_string()))
}

fn validate_context(
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
) -> Result<()> {
    let expected = document_type_for_action(action);
    if context.business_object_type != expected.as_str() {
        return Err(Error::ConflictError(
            "审批领域动作与冻结单据类型不一致".to_string(),
        ));
    }
    if context.actor_id != actor.id() {
        return Err(Error::Forbidden("审批领域动作操作人与认证身份不一致".to_string()));
    }
    Ok(())
}

fn document_type_for_action(action: ApprovalDomainAction) -> DocumentType {
    match action {
        ApprovalDomainAction::SalesOrderStartApprovalSubmission
        | ApprovalDomainAction::SalesOrderFormalizeApprovedSubmission
        | ApprovalDomainAction::SalesOrderCancelApprovalSubmission => DocumentType::SalesOrder,
        ApprovalDomainAction::VoucherSalesOrderStartApprovalSubmission
        | ApprovalDomainAction::VoucherSalesOrderFormalizeApprovedSubmission
        | ApprovalDomainAction::VoucherSalesOrderCancelApprovalSubmission => DocumentType::VoucherSalesOrder,
        ApprovalDomainAction::SalesChangeOrderSubmitSalesChange
        | ApprovalDomainAction::SalesChangeOrderApplyEffectiveChange
        | ApprovalDomainAction::SalesChangeOrderCancelApproval => DocumentType::SalesChangeOrder,
        ApprovalDomainAction::PurchaseOrderSubmit
        | ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder
        | ApprovalDomainAction::PurchaseOrderCancelApproval => DocumentType::PurchaseOrder,
        ApprovalDomainAction::PurchaseChangeOrderSubmitChange
        | ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange
        | ApprovalDomainAction::PurchaseChangeOrderCancelApproval => DocumentType::PurchaseChangeOrder,
        ApprovalDomainAction::StockAdjustmentSubmit
        | ApprovalDomainAction::StockAdjustmentPost
        | ApprovalDomainAction::StockAdjustmentCancelApproval => DocumentType::StockAdjustment,
        ApprovalDomainAction::CustomerReceiptSubmit
        | ApprovalDomainAction::CustomerReceiptPost
        | ApprovalDomainAction::CustomerReceiptCancelApproval => DocumentType::CustomerReceipt,
        ApprovalDomainAction::CustomerRefundSubmit
        | ApprovalDomainAction::CustomerRefundPost
        | ApprovalDomainAction::CustomerRefundCancelApproval => DocumentType::CustomerRefund,
        ApprovalDomainAction::SupplierRefundSubmit
        | ApprovalDomainAction::SupplierRefundPost
        | ApprovalDomainAction::SupplierRefundCancelApproval => DocumentType::SupplierRefund,
        ApprovalDomainAction::ReceiptReversalSubmit
        | ApprovalDomainAction::ReceiptReversalPost
        | ApprovalDomainAction::ReceiptReversalCancelApproval => DocumentType::ReceiptReversal,
        ApprovalDomainAction::PaymentReversalSubmit
        | ApprovalDomainAction::PaymentReversalPost
        | ApprovalDomainAction::PaymentReversalCancelApproval => DocumentType::PaymentReversal,
    }
}
