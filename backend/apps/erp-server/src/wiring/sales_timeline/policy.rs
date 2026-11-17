use erp_permissions::{
    Permission, fulfillment, internal, inventory, payable, purchase_order, purchase_payment_request,
    receivable, returns, sales_order,
};
use erp_workflow::DocumentType;

/// 组合已有详情入口的读取策略；退货单不属于审批来源适配器。
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadPolicy {
    Invoice,
    PaymentRequest,
    SalesReturn,
    PurchaseReturn,
    Fulfillment,
    ApprovalSource,
}

impl ReadPolicy {
    pub(super) fn for_document(kind: DocumentType) -> Self {
        use DocumentType::*;
        match kind {
            Invoice => Self::Invoice,
            PurchasePaymentRequest => Self::PaymentRequest,
            SalesReturnCase => Self::SalesReturn,
            PurchaseReturnOrder => Self::PurchaseReturn,
            Delivery | ElectronicDelivery | ServiceFulfillment | PurchaseReceipt | CustomerAcceptance => {
                Self::Fulfillment
            },
            _ => Self::ApprovalSource,
        }
    }

    pub(super) fn permission(&self, kind: DocumentType) -> Permission {
        use DocumentType::*;
        match kind {
            SalesOrder | VoucherSalesOrder => sales_order::DETAIL,
            SalesChangeOrder => internal::SALES_CHANGE_DETAIL,
            PurchaseOrder => purchase_order::DETAIL,
            PurchaseChangeOrder => purchase_order::PURCHASE_CHANGE_DETAIL,
            PurchaseReceipt => fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
            Delivery => fulfillment::DELIVERY_DELIVERY_DETAIL,
            ElectronicDelivery => fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            ServiceFulfillment => fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            CustomerAcceptance => fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL,
            StockAdjustment => inventory::STOCK_ADJUSTMENT_DETAIL,
            CustomerReceipt => receivable::CUSTOMER_RECEIPT_DETAIL,
            SalesInvoiceRequest => receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
            PurchasePaymentRequest => purchase_payment_request::DETAIL,
            SupplierPayment => payable::SUPPLIER_PAYMENT_DETAIL,
            Invoice => receivable::INVOICE_DETAIL,
            SalesReturnCase => returns::SALES_RETURN_SALES_RETURN_CASE_DETAIL,
            PurchaseReturnOrder => returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL,
            CustomerRefund => returns::CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL,
            SupplierRefund => returns::SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL,
            ReceiptReversal => returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL,
            PaymentReversal => returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_documents_use_their_own_detail_policy() {
        for (kind, expected, grant) in [
            (DocumentType::SalesReturnCase, ReadPolicy::SalesReturn, "sales_return_case:detail"),
            (DocumentType::PurchaseReturnOrder, ReadPolicy::PurchaseReturn, "purchase_return_order:detail"),
        ] {
            let policy = ReadPolicy::for_document(kind);
            assert_eq!(policy, expected);
            let permission = policy.permission(kind);
            assert!(Permission::parse(grant).unwrap().covers(&permission));
            assert!(!Permission::parse("sales_order:detail").unwrap().covers(&permission));
        }
    }

    #[test]
    fn fulfillment_accepts_the_exact_registered_read_grant() {
        for (kind, grant) in [
            (DocumentType::ElectronicDelivery, "electronic_delivery:list"),
            (DocumentType::ServiceFulfillment, "service_fulfillment:list"),
            (DocumentType::Delivery, "delivery:detail"),
            (DocumentType::PurchaseReceipt, "purchase_receipt:detail"),
            (DocumentType::CustomerAcceptance, "customer_acceptance:detail"),
        ] {
            let policy = ReadPolicy::for_document(kind);
            assert_eq!(policy, ReadPolicy::Fulfillment);
            let permission = policy.permission(kind);
            assert!(Permission::parse(grant).unwrap().covers(&permission));
            assert!(!Permission::parse("sales_order:detail").unwrap().covers(&permission));
        }
    }
}
