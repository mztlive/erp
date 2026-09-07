//! Domain object-read branches for approval binding.

use erp_workflow::entity::document_registry::DocumentType as WorkflowDocumentType;
use erp_workflow::ports::ApprovalObjectReadPort;
use erp_workflow::service::approval::business_adapter::{ApprovalAdapterSpec, BindingRevalidationContext};
use erp_workflow::{Error, Result};

/// Process-owned object-read adapter that calls remaining domain services.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessObjectRead;

impl ApprovalObjectReadPort for ProcessObjectRead {
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

/// Domain-wired object-read decision used by approval binding.
///
/// # Parameters
/// * `spec` - complete adapter spec
/// * `context` - document organization and creator
/// * `assignee_user_id` - candidate approver
///
/// # Returns
/// `Some` when the domain adapter is wired.
///
/// # Errors
/// Empty organization/assignee or domain adapter failures.
pub fn adapter_object_read_decision(
    spec: &ApprovalAdapterSpec,
    context: &BindingRevalidationContext,
    assignee_user_id: &str,
) -> Result<Option<bool>> {
    adapter_object_read_for_type(spec.document_type, &context.organization_id, assignee_user_id)
}

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
            services::sales_order::sales_order_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::SalesChangeOrder => Ok(Some(
            services::sales_review::sales_change_order_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::PurchaseOrder => Ok(Some(
            services::purchase_order::purchase_order_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::PurchaseChangeOrder => Ok(Some(
            services::purchase_order::purchase_change_order_object_readable(
                organization_id,
                assignee_user_id,
            )
            .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::CustomerReceipt => Ok(Some(
            crate::finance_posting::receivable::customer_receipt_object_readable(
                organization_id,
                assignee_user_id,
            )
            .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::CustomerRefund => Ok(Some(
            services::returns::customer_refund_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::SupplierRefund => Ok(Some(
            services::returns::supplier_refund_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::ReceiptReversal => Ok(Some(
            services::returns::receipt_reversal_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        WorkflowDocumentType::PaymentReversal => Ok(Some(
            services::returns::payment_reversal_object_readable(organization_id, assignee_user_id)
                .map_err(map_workflow_error)?,
        )),
        _ => Ok(None),
    }
}

/// Unwired object-read must fail closed.
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

/// Map remaining domain service errors onto workflow error classes.
///
/// # 参数
/// * `error` - 旧 services 错误
///
/// # 返回
/// 返回 workflow 错误。
///
/// # 错误
/// 无；调用方继续传播映射后的错误。
fn map_workflow_error(error: services::Error) -> Error {
    match error {
        services::Error::ValidationError(message) => Error::ValidationError(message),
        services::Error::BusinessLogicError(message) => Error::BusinessLogicError(message),
        other => Error::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::service::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
    use erp_workflow::DocumentType;

    use super::adapter_object_read_decision;

    /// 已迁出的领域读取分支对有效组织/审批人返回显式 true。
    #[test]
    fn wired_domain_types_return_explicit_read_decision() {
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };
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
            adapter_object_read_decision(&stock, &context, "u1").expect("库存调整本阶段仍未接线"),
            None
        );
    }
}
