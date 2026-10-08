//! 审批绑定使用的领域对象读取分支。

use erp_workflow::entity::document_registry::DocumentType as WorkflowDocumentType;
use erp_workflow::ports::ApprovalObjectReadPort;
use erp_workflow::service::approval::business_adapter::{ApprovalAdapterSpec, BindingRevalidationContext};
use erp_workflow::{Error, Result};

/// 调用仍留在领域侧的对象读取判定。
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessObjectRead;

impl ApprovalObjectReadPort for ProcessObjectRead {
    /// 创建人不参与读取权；只按单据类型、组织和审批人判定。
    ///
    /// # 参数
    /// * `document_type` - 审批绑定的单据类型。
    /// * `organization_id` - 单据组织。
    /// * `creator_id` - 创建人。本实现不使用。
    /// * `assignee_user_id` - 候选审批人。
    ///
    /// # 返回
    /// 已接线时返回 `Some`，布尔值是读取权；未接线的单据类型返回 `None`。开票申请和库存调整在组织与审批人非空时返回 `Some(true)`。
    ///
    /// # 错误
    /// 组织或审批人为空时返回校验错误；领域读取失败时返回映射后的工作流错误。
    fn object_read_decision(
        &self,
        document_type: WorkflowDocumentType,
        organization_id: &str,
        creator_id: &str,
        assignee_user_id: &str,
    ) -> erp_workflow::Result<Option<bool>> {
        let _ = creator_id;
        adapter_object_read_for_type(document_type, organization_id, assignee_user_id)
    }
}

/// 按适配器单据类型给出审批绑定使用的对象读取权。
///
/// 创建人不参与判定。开票申请和库存调整在组织与审批人非空时直接允许读取。
///
/// # 参数
/// * `spec` - 含单据类型的适配器规格。
/// * `context` - 单据组织与创建人；本函数只使用组织。
/// * `assignee_user_id` - 候选审批人。
///
/// # 返回
/// 已接线时返回 `Some`，布尔值是读取权；未接线的单据类型返回 `None`。
///
/// # 错误
/// 组织或审批人为空时返回校验错误；领域读取失败时返回映射后的工作流错误。
pub fn adapter_object_read_decision(
    spec: &ApprovalAdapterSpec,
    context: &BindingRevalidationContext,
    assignee_user_id: &str,
) -> Result<Option<bool>> {
    adapter_object_read_for_type(spec.document_type, &context.organization_id, assignee_user_id)
}

/// 空组织或审批人拒绝；未接线的单据类型返回 `None`，不得默认放行。
fn adapter_object_read_for_type(
    document_type: WorkflowDocumentType,
    organization_id: &str,
    assignee_user_id: &str,
) -> Result<Option<bool>> {
    if organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    match document_type {
        WorkflowDocumentType::SalesOrder | WorkflowDocumentType::VoucherSalesOrder => Ok(Some(
            crate::order_to_cash::sales_order_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::SalesChangeOrder => Ok(Some(
            crate::sales_change::sales_change_order_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::PurchaseOrder => Ok(Some(
            crate::procure_to_pay::purchase_order_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::PurchaseChangeOrder => Ok(Some(
            crate::procure_to_pay::purchase_change_order_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::SalesInvoiceRequest | WorkflowDocumentType::StockAdjustment => Ok(Some(true)),
        WorkflowDocumentType::CustomerReceipt => Ok(Some(
            crate::finance_posting::receivable::customer_receipt_object_readable(
                organization_id,
                assignee_user_id,
            )
            .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::CustomerRefund => Ok(Some(
            crate::reverse_flow::customer_refund_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::SupplierRefund => Ok(Some(
            crate::reverse_flow::supplier_refund_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::ReceiptReversal => Ok(Some(
            crate::reverse_flow::receipt_reversal_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::PaymentReversal => Ok(Some(
            crate::reverse_flow::payment_reversal_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        _ => Ok(None),
    }
}

/// 未接线的对象读取必须失败关闭，禁止把 `None` 当成允许。
///
/// # 参数
/// * `decision` - 领域 Adapter 返回的显式判定
///
/// # 返回
/// 已接线时返回布尔读取权。
///
/// # 错误
/// `None` 表示 Adapter 未接线，禁止默认放行。
pub fn require_wired_object_read(decision: Option<bool>) -> Result<bool> {
    decision.ok_or_else(|| Error::ValidationError("对象读取权未接线，已按安全策略拒绝".to_string()))
}

/// 把流程边界错误映射为工作流错误类别。
///
/// # 参数
/// * `error` - 流程边界错误
///
/// # 返回
/// 校验和业务逻辑错误保留原文，其余收成内部错误。
///
/// # 错误
/// 不返回错误。
fn map_workflow_error(error: crate::Error) -> Error {
    match error {
        crate::Error::ValidationError(message) => Error::ValidationError(message),
        crate::Error::BusinessLogicError(message) => Error::BusinessLogicError(message),
        other => Error::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::DocumentType;
    use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};

    use super::adapter_object_read_decision;

    /// 已迁出的领域读取分支对有效组织/审批人返回显式 true。
    #[test]
    fn wired_domain_types_return_explicit_read_decision() {
        let context = BindingRevalidationContext::new("org-1".to_string(), "creator-1".to_string());
        let sales = adapter_spec_of(DocumentType::SalesOrder).expect("销售单必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&sales, &context, "u1").expect("销售单读取权已接线"),
            Some(true)
        );
        let payment_reversal = adapter_spec_of(DocumentType::PaymentReversal).expect("付款冲正必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&payment_reversal, &context, "u1").expect("付款冲正读取权已接线"),
            Some(true)
        );
        let stock = adapter_spec_of(DocumentType::StockAdjustment).expect("试点必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&stock, &context, "u1").expect("库存调整对象读取已接线"),
            Some(true)
        );
    }
}
