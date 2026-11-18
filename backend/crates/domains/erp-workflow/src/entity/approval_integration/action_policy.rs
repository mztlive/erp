//! ERP 审批集成动作与单据类型的强类型合同。

use crate::entity::document_registry::DocumentType;

// 每个动作只登记一次；枚举、穷尽集合、所属单据与审计代码由同一声明生成。
macro_rules! approval_actions {
    ($($document:ident { $($action:ident => $code:literal),+ $(,)? }),+ $(,)?) => {
        /// ERP 审批政策登记的强类型领域动作。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum ApprovalDomainAction {
            $($($action,)+)+
        }

        impl ApprovalDomainAction {
            /// 全部已登记审批领域动作的稳定穷尽集合。
            pub const ALL: [Self; 52] = [$($(Self::$action,)+)+];

            /// 从同一政策登记取出动作唯一所属的 ERP 单据类型，用于核对动作与单据是否匹配。
            pub fn document_type(self) -> DocumentType {
                match self { $($(Self::$action => DocumentType::$document,)+)+ }
            }

            /// 取出政策登记的 `Service::method` 形式动作码，供审计稳定识别该领域动作。
            pub fn as_str(self) -> &'static str {
                match self { $($(Self::$action => $code,)+)+ }
            }
        }
    };
}

approval_actions! {
    SalesOrder {
        SalesOrderStartApprovalSubmission => "SalesOrderService::start_approval_submission",
        SalesOrderFormalizeApprovedSubmission => "SalesOrderService::formalize_approved_submission",
        SalesOrderCancelApprovalSubmission => "SalesOrderService::cancel_approval_submission",
        SalesOrderReject => "SalesOrderProcess::reject",
    },
    VoucherSalesOrder {
        VoucherSalesOrderStartApprovalSubmission => "SalesOrderService::start_approval_submission",
        VoucherSalesOrderFormalizeApprovedSubmission => "SalesOrderService::formalize_approved_submission",
        VoucherSalesOrderCancelApprovalSubmission => "SalesOrderService::cancel_approval_submission",
        VoucherSalesOrderReject => "VoucherSalesOrderProcess::reject",
    },
    SalesChangeOrder {
        SalesChangeOrderSubmitSalesChange => "SalesChangeOrderService::submit_sales_change",
        SalesChangeOrderApplyEffectiveChange => "SalesChangeOrderService::apply_effective_change",
        SalesChangeOrderCancelApproval => "SalesChangeOrderService::cancel_approval",
        SalesChangeOrderReject => "SalesChangeOrderProcess::reject",
    },
    PurchaseOrder {
        PurchaseOrderSubmit => "PurchaseOrderService::submit",
        PurchaseOrderFormalizeApprovedOrder => "PurchaseOrderService::formalize_approved_order",
        PurchaseOrderCancelApproval => "PurchaseOrderService::cancel_approval",
        PurchaseOrderReject => "PurchaseOrderProcess::reject",
    },
    PurchaseChangeOrder {
        PurchaseChangeOrderSubmitChange => "PurchaseChangeService::submit_change",
        PurchaseChangeOrderApplyEffectiveChange => "PurchaseChangeService::apply_effective_change",
        PurchaseChangeOrderCancelApproval => "PurchaseChangeService::cancel_approval",
        PurchaseChangeOrderReject => "PurchaseChangeOrderProcess::reject",
    },
    StockAdjustment {
        StockAdjustmentSubmit => "InventoryService::submit_stock_adjustment",
        StockAdjustmentPost => "InventoryService::post_stock_adjustment",
        StockAdjustmentCancelApproval => "InventoryService::cancel_stock_adjustment_approval",
        StockAdjustmentReject => "StockAdjustmentProcess::reject",
    },
    SalesInvoiceRequest {
        SalesInvoiceRequestSubmit => "InvoiceRequestProcess::submit",
        SalesInvoiceRequestApprove => "InvoiceRequestProcess::approve",
        SalesInvoiceRequestCancelApproval => "InvoiceRequestProcess::cancel",
        SalesInvoiceRequestReject => "SalesInvoiceRequestProcess::reject",
    },
    PurchasePaymentRequest {
        PurchasePaymentRequestSubmit => "PurchasePaymentRequestProcess::submit",
        PurchasePaymentRequestApprove => "PurchasePaymentRequestProcess::approve",
        PurchasePaymentRequestReject => "PurchasePaymentRequestProcess::reject",
        PurchasePaymentRequestCancelApproval => "PurchasePaymentRequestProcess::cancel",
    },
    CustomerReceipt {
        CustomerReceiptSubmit => "ReceivableService::submit_customer_receipt",
        CustomerReceiptPost => "ReceivableService::post_customer_receipt",
        CustomerReceiptCancelApproval => "ReceivableService::cancel_customer_receipt_approval",
        CustomerReceiptReject => "CustomerReceiptProcess::reject",
    },
    CustomerRefund {
        CustomerRefundSubmit => "ReturnsService::submit_customer_refund",
        CustomerRefundPost => "ReturnsService::post_customer_refund",
        CustomerRefundCancelApproval => "ReturnsService::cancel_customer_refund_approval",
        CustomerRefundReject => "CustomerRefundProcess::reject",
    },
    SupplierRefund {
        SupplierRefundSubmit => "ReturnsService::submit_supplier_refund",
        SupplierRefundPost => "ReturnsService::post_supplier_refund",
        SupplierRefundCancelApproval => "ReturnsService::cancel_supplier_refund_approval",
        SupplierRefundReject => "SupplierRefundProcess::reject",
    },
    ReceiptReversal {
        ReceiptReversalSubmit => "ReturnsService::submit_receipt_reversal",
        ReceiptReversalPost => "ReturnsService::post_receipt_reversal",
        ReceiptReversalCancelApproval => "ReturnsService::cancel_receipt_reversal_approval",
        ReceiptReversalReject => "ReceiptReversalProcess::reject",
    },
    PaymentReversal {
        PaymentReversalSubmit => "ReturnsService::submit_payment_reversal",
        PaymentReversalPost => "ReturnsService::post_payment_reversal",
        PaymentReversalCancelApproval => "ReturnsService::cancel_payment_reversal_approval",
        PaymentReversalReject => "PaymentReversalProcess::reject",
    },
}

impl ApprovalDomainAction {
    /// 三类审批动作两两互异时返回 `true`。
    /// 三个动作互不相同时返回 `true`，任两个相同时返回 `false`。
    pub fn are_distinct(first: Self, second: Self, third: Self) -> bool {
        first != second && first != third && second != third
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::ApprovalDomainAction;

    #[test]
    fn all_actions_have_document_type_and_stable_code() {
        assert_eq!(ApprovalDomainAction::ALL.len(), 52);
        let actions = ApprovalDomainAction::ALL.into_iter().collect::<HashSet<_>>();
        assert_eq!(actions.len(), 52);
        for action in actions {
            assert!(!action.document_type().as_str().is_empty());
            assert!(!action.as_str().is_empty());
        }
    }

    #[test]
    fn distinct_triple_rejects_any_collision() {
        use super::ApprovalDomainAction as Action;

        assert!(Action::are_distinct(
            Action::StockAdjustmentSubmit,
            Action::StockAdjustmentPost,
            Action::StockAdjustmentCancelApproval,
        ));
        assert!(!Action::are_distinct(
            Action::StockAdjustmentSubmit,
            Action::StockAdjustmentSubmit,
            Action::StockAdjustmentCancelApproval,
        ));
        assert!(!Action::are_distinct(
            Action::StockAdjustmentSubmit,
            Action::StockAdjustmentPost,
            Action::StockAdjustmentPost,
        ));
        assert!(!Action::are_distinct(
            Action::StockAdjustmentPost,
            Action::StockAdjustmentSubmit,
            Action::StockAdjustmentPost,
        ));
    }
}
