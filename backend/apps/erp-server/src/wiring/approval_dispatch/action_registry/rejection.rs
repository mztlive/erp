//! 终态驳回的领域接线；所有写入复用运行时的唯一事务。

use erp_order_to_cash::Error as SalesError;
use erp_sales::service::{sales_order, sales_review};

use super::{
    ApprovalActionContext, ApprovalActionRegistry, ApprovalDomainAction, AuditActor, Executor, ServiceError,
    ServiceResult, finance_posting, inventory_adjustment_process, procure_to_pay, rejected_action,
    require_transaction, reverse_flow,
};

// 执行已由运行时证明身份和冻结版本的领域驳回动作。
// `action` / `context` 为注册动作及冻结审批身份，`executor` 为当前决定的事务。
// 领域状态及本次审批占用更新成功时返回。
// 缺少事务、动作不匹配或领域不变式失败时返回原错误。
pub(super) async fn dispatch(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    require_transaction(executor)?;
    let id = context.business_object_id();
    let version = context
        .subject_version()
        .parse::<u32>()
        .map_err(|_| ServiceError::ConflictError("审批提交版本无效".into()))?;
    match action {
        ApprovalDomainAction::SalesOrderReject
        | ApprovalDomainAction::VoucherSalesOrderReject
        | ApprovalDomainAction::SalesChangeOrderReject => {
            reject_sales(registry, action, context, actor, executor).await
        },
        ApprovalDomainAction::PurchaseOrderReject => procure_to_pay::reject_order_approval(
            &registry.db,
            id,
            version,
            context.approval_process_instance_id(),
            actor,
            executor,
        )
        .await
        .map_err(Into::into),
        ApprovalDomainAction::PurchaseChangeOrderReject => procure_to_pay::reject_change_approval(
            &registry.db,
            id,
            version,
            context.approval_process_instance_id(),
            actor,
            executor,
        )
        .await
        .map_err(Into::into),
        _ => reject_finance_and_inventory(registry, action, context, version, actor, executor).await,
    }
}

async fn reject_finance_and_inventory(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    version: u32,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    let id = context.business_object_id();
    match action {
        ApprovalDomainAction::StockAdjustmentReject => inventory_adjustment_process(
            registry.db.clone(),
            registry.rbac.clone(),
            registry.authorization.clone(),
        )
        .reject(id, version, actor, executor)
        .await
        .map_err(Into::into),
        ApprovalDomainAction::SalesInvoiceRequestReject => {
            finance_posting::receivable::invoice_request::reject(&registry.db, id, version, actor, executor)
                .await
                .map_err(Into::into)
        },
        ApprovalDomainAction::CustomerReceiptReject => {
            finance_posting::receivable::reject_customer_receipt_approval_apply(
                &registry.db,
                id,
                version,
                actor,
                executor,
            )
            .await
            .map_err(Into::into)
        },
        ApprovalDomainAction::CustomerRefundReject
        | ApprovalDomainAction::SupplierRefundReject
        | ApprovalDomainAction::ReceiptReversalReject
        | ApprovalDomainAction::PaymentReversalReject => reverse_flow::reject_approval(
            &registry.db,
            action.document_type(),
            id,
            version,
            action,
            actor,
            executor,
        )
        .await
        .map_err(Into::into),
        _ => rejected_action(action),
    }
}

// 执行销售驳回动作时先核对冻结提交序号，再把原审批事务和身份传给销售领域。
async fn reject_sales(
    registry: &ApprovalActionRegistry,
    action: ApprovalDomainAction,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> ServiceResult<()> {
    let version = context
        .subject_version()
        .parse::<u32>()
        .map_err(|_| ServiceError::ConflictError("销售审批提交版本无效".into()))?;
    let result = if action == ApprovalDomainAction::SalesChangeOrderReject {
        sales_review::reject_change(&registry.db, context.business_object_id(), version, actor.id(), executor)
            .await
    } else {
        sales_order::reject_order(&registry.db, context.business_object_id(), version, actor.id(), executor)
            .await
    };
    result.map_err(SalesError::from).map_err(Into::into)
}
