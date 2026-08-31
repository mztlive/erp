//! ERP 审批集成动作与单据类型的强类型合同。

use crate::document_registry::DocumentType;

/// ERP 审批政策登记的强类型领域动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApprovalDomainAction {
    SalesOrderStartApprovalSubmission,
    SalesOrderFormalizeApprovedSubmission,
    SalesOrderCancelApprovalSubmission,
    VoucherSalesOrderStartApprovalSubmission,
    VoucherSalesOrderFormalizeApprovedSubmission,
    VoucherSalesOrderCancelApprovalSubmission,
    SalesChangeOrderSubmitSalesChange,
    SalesChangeOrderApplyEffectiveChange,
    SalesChangeOrderCancelApproval,
    PurchaseOrderSubmit,
    PurchaseOrderFormalizeApprovedOrder,
    PurchaseOrderCancelApproval,
    PurchaseChangeOrderSubmitChange,
    PurchaseChangeOrderApplyEffectiveChange,
    PurchaseChangeOrderCancelApproval,
    StockAdjustmentSubmit,
    StockAdjustmentPost,
    StockAdjustmentCancelApproval,
    CustomerReceiptSubmit,
    CustomerReceiptPost,
    CustomerReceiptCancelApproval,
    CustomerRefundSubmit,
    CustomerRefundPost,
    CustomerRefundCancelApproval,
    SupplierRefundSubmit,
    SupplierRefundPost,
    SupplierRefundCancelApproval,
    ReceiptReversalSubmit,
    ReceiptReversalPost,
    ReceiptReversalCancelApproval,
    PaymentReversalSubmit,
    PaymentReversalPost,
    PaymentReversalCancelApproval,
}

impl ApprovalDomainAction {
    /// 全部已登记审批领域动作的稳定穷尽集合。
    pub const ALL: [Self; 33] = [
        Self::SalesOrderStartApprovalSubmission,
        Self::SalesOrderFormalizeApprovedSubmission,
        Self::SalesOrderCancelApprovalSubmission,
        Self::VoucherSalesOrderStartApprovalSubmission,
        Self::VoucherSalesOrderFormalizeApprovedSubmission,
        Self::VoucherSalesOrderCancelApprovalSubmission,
        Self::SalesChangeOrderSubmitSalesChange,
        Self::SalesChangeOrderApplyEffectiveChange,
        Self::SalesChangeOrderCancelApproval,
        Self::PurchaseOrderSubmit,
        Self::PurchaseOrderFormalizeApprovedOrder,
        Self::PurchaseOrderCancelApproval,
        Self::PurchaseChangeOrderSubmitChange,
        Self::PurchaseChangeOrderApplyEffectiveChange,
        Self::PurchaseChangeOrderCancelApproval,
        Self::StockAdjustmentSubmit,
        Self::StockAdjustmentPost,
        Self::StockAdjustmentCancelApproval,
        Self::CustomerReceiptSubmit,
        Self::CustomerReceiptPost,
        Self::CustomerReceiptCancelApproval,
        Self::CustomerRefundSubmit,
        Self::CustomerRefundPost,
        Self::CustomerRefundCancelApproval,
        Self::SupplierRefundSubmit,
        Self::SupplierRefundPost,
        Self::SupplierRefundCancelApproval,
        Self::ReceiptReversalSubmit,
        Self::ReceiptReversalPost,
        Self::ReceiptReversalCancelApproval,
        Self::PaymentReversalSubmit,
        Self::PaymentReversalPost,
        Self::PaymentReversalCancelApproval,
    ];

    /// 返回动作所属的唯一 ERP 单据类型。
    pub fn document_type(self) -> DocumentType {
        match self {
            Self::SalesOrderStartApprovalSubmission
            | Self::SalesOrderFormalizeApprovedSubmission
            | Self::SalesOrderCancelApprovalSubmission => DocumentType::SalesOrder,
            Self::VoucherSalesOrderStartApprovalSubmission
            | Self::VoucherSalesOrderFormalizeApprovedSubmission
            | Self::VoucherSalesOrderCancelApprovalSubmission => DocumentType::VoucherSalesOrder,
            Self::SalesChangeOrderSubmitSalesChange
            | Self::SalesChangeOrderApplyEffectiveChange
            | Self::SalesChangeOrderCancelApproval => DocumentType::SalesChangeOrder,
            Self::PurchaseOrderSubmit
            | Self::PurchaseOrderFormalizeApprovedOrder
            | Self::PurchaseOrderCancelApproval => DocumentType::PurchaseOrder,
            Self::PurchaseChangeOrderSubmitChange
            | Self::PurchaseChangeOrderApplyEffectiveChange
            | Self::PurchaseChangeOrderCancelApproval => DocumentType::PurchaseChangeOrder,
            Self::StockAdjustmentSubmit | Self::StockAdjustmentPost | Self::StockAdjustmentCancelApproval => {
                DocumentType::StockAdjustment
            }
            Self::CustomerReceiptSubmit | Self::CustomerReceiptPost | Self::CustomerReceiptCancelApproval => {
                DocumentType::CustomerReceipt
            }
            Self::CustomerRefundSubmit | Self::CustomerRefundPost | Self::CustomerRefundCancelApproval => {
                DocumentType::CustomerRefund
            }
            Self::SupplierRefundSubmit | Self::SupplierRefundPost | Self::SupplierRefundCancelApproval => {
                DocumentType::SupplierRefund
            }
            Self::ReceiptReversalSubmit | Self::ReceiptReversalPost | Self::ReceiptReversalCancelApproval => {
                DocumentType::ReceiptReversal
            }
            Self::PaymentReversalSubmit | Self::PaymentReversalPost | Self::PaymentReversalCancelApproval => {
                DocumentType::PaymentReversal
            }
        }
    }

    /// 返回稳定动作代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SalesOrderStartApprovalSubmission => "SalesOrderService::start_approval_submission",
            Self::SalesOrderFormalizeApprovedSubmission => "SalesOrderService::formalize_approved_submission",
            Self::SalesOrderCancelApprovalSubmission => "SalesOrderService::cancel_approval_submission",
            Self::VoucherSalesOrderStartApprovalSubmission => "SalesOrderService::start_approval_submission",
            Self::VoucherSalesOrderFormalizeApprovedSubmission => {
                "SalesOrderService::formalize_approved_submission"
            }
            Self::VoucherSalesOrderCancelApprovalSubmission => {
                "SalesOrderService::cancel_approval_submission"
            }
            Self::SalesChangeOrderSubmitSalesChange => "SalesChangeOrderService::submit_sales_change",
            Self::SalesChangeOrderApplyEffectiveChange => "SalesChangeOrderService::apply_effective_change",
            Self::SalesChangeOrderCancelApproval => "SalesChangeOrderService::cancel_approval",
            Self::PurchaseOrderSubmit => "PurchaseOrderService::submit",
            Self::PurchaseOrderFormalizeApprovedOrder => "PurchaseOrderService::formalize_approved_order",
            Self::PurchaseOrderCancelApproval => "PurchaseOrderService::cancel_approval",
            Self::PurchaseChangeOrderSubmitChange => "PurchaseChangeService::submit_change",
            Self::PurchaseChangeOrderApplyEffectiveChange => "PurchaseChangeService::apply_effective_change",
            Self::PurchaseChangeOrderCancelApproval => "PurchaseChangeService::cancel_approval",
            Self::StockAdjustmentSubmit => "InventoryService::submit_stock_adjustment",
            Self::StockAdjustmentPost => "InventoryService::post_stock_adjustment",
            Self::StockAdjustmentCancelApproval => "InventoryService::cancel_stock_adjustment_approval",
            Self::CustomerReceiptSubmit => "ReceivableService::submit_customer_receipt",
            Self::CustomerReceiptPost => "ReceivableService::post_customer_receipt",
            Self::CustomerReceiptCancelApproval => "ReceivableService::cancel_customer_receipt_approval",
            Self::CustomerRefundSubmit => "ReturnsService::submit_customer_refund",
            Self::CustomerRefundPost => "ReturnsService::post_customer_refund",
            Self::CustomerRefundCancelApproval => "ReturnsService::cancel_customer_refund_approval",
            Self::SupplierRefundSubmit => "ReturnsService::submit_supplier_refund",
            Self::SupplierRefundPost => "ReturnsService::post_supplier_refund",
            Self::SupplierRefundCancelApproval => "ReturnsService::cancel_supplier_refund_approval",
            Self::ReceiptReversalSubmit => "ReturnsService::submit_receipt_reversal",
            Self::ReceiptReversalPost => "ReturnsService::post_receipt_reversal",
            Self::ReceiptReversalCancelApproval => "ReturnsService::cancel_receipt_reversal_approval",
            Self::PaymentReversalSubmit => "ReturnsService::submit_payment_reversal",
            Self::PaymentReversalPost => "ReturnsService::post_payment_reversal",
            Self::PaymentReversalCancelApproval => "ReturnsService::cancel_payment_reversal_approval",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::ApprovalDomainAction;

    #[test]
    fn all_actions_have_document_type_and_stable_code() {
        assert_eq!(ApprovalDomainAction::ALL.len(), 33);
        let actions = ApprovalDomainAction::ALL.into_iter().collect::<HashSet<_>>();
        assert_eq!(actions.len(), 33);
        for action in actions {
            assert!(!action.document_type().as_str().is_empty());
            assert!(!action.as_str().is_empty());
        }
    }
}
