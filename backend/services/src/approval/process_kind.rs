//! `DocumentType` 与 `ProcessKind` 的双向一对一穷尽映射。
//!
//! 映射按合同 §4.3 的 20 个固定单据类型实现。不得使用可缺项映射表，
//! 也不得接受客户端或数据库任意字符串推导未登记类型。

use bpm::ProcessKind;
use entities::document_registry::DocumentType;

/// 将 ERP 单据类型映射为 BPM 流程种类。
///
/// # 参数
/// * `document_type` - 合同 §4.3 固定单据类型
///
/// # 返回
/// 返回与该单据类型一一对应的流程种类。
pub fn process_kind_of(document_type: DocumentType) -> ProcessKind {
    match document_type {
        DocumentType::SalesOrder => ProcessKind::SalesOrder,
        DocumentType::VoucherSalesOrder => ProcessKind::VoucherSalesOrder,
        DocumentType::SalesChangeOrder => ProcessKind::SalesChangeOrder,
        DocumentType::PurchaseOrder => ProcessKind::PurchaseOrder,
        DocumentType::PurchaseChangeOrder => ProcessKind::PurchaseChangeOrder,
        DocumentType::StockAdjustment => ProcessKind::StockAdjustment,
        DocumentType::CustomerReceipt => ProcessKind::CustomerReceipt,
        DocumentType::SupplierPayment => ProcessKind::SupplierPayment,
        DocumentType::CustomerRefund => ProcessKind::CustomerRefund,
        DocumentType::SupplierRefund => ProcessKind::SupplierRefund,
        DocumentType::ReceiptReversal => ProcessKind::ReceiptReversal,
        DocumentType::PaymentReversal => ProcessKind::PaymentReversal,
        DocumentType::PurchaseReceipt => ProcessKind::PurchaseReceipt,
        DocumentType::Delivery => ProcessKind::Delivery,
        DocumentType::ElectronicDelivery => ProcessKind::ElectronicDelivery,
        DocumentType::ServiceFulfillment => ProcessKind::ServiceFulfillment,
        DocumentType::CustomerAcceptance => ProcessKind::CustomerAcceptance,
        DocumentType::Invoice => ProcessKind::Invoice,
        DocumentType::SalesReturnCase => ProcessKind::SalesReturnCase,
        DocumentType::PurchaseReturnOrder => ProcessKind::PurchaseReturnOrder,
    }
}

/// 将 BPM 流程种类映射回 ERP 单据类型。
///
/// # 参数
/// * `process_kind` - 已冻结的流程种类
///
/// # 返回
/// 返回与该流程种类一一对应的单据类型。
pub fn document_type_of(process_kind: ProcessKind) -> DocumentType {
    match process_kind {
        ProcessKind::SalesOrder => DocumentType::SalesOrder,
        ProcessKind::VoucherSalesOrder => DocumentType::VoucherSalesOrder,
        ProcessKind::SalesChangeOrder => DocumentType::SalesChangeOrder,
        ProcessKind::PurchaseOrder => DocumentType::PurchaseOrder,
        ProcessKind::PurchaseChangeOrder => DocumentType::PurchaseChangeOrder,
        ProcessKind::StockAdjustment => DocumentType::StockAdjustment,
        ProcessKind::CustomerReceipt => DocumentType::CustomerReceipt,
        ProcessKind::SupplierPayment => DocumentType::SupplierPayment,
        ProcessKind::CustomerRefund => DocumentType::CustomerRefund,
        ProcessKind::SupplierRefund => DocumentType::SupplierRefund,
        ProcessKind::ReceiptReversal => DocumentType::ReceiptReversal,
        ProcessKind::PaymentReversal => DocumentType::PaymentReversal,
        ProcessKind::PurchaseReceipt => DocumentType::PurchaseReceipt,
        ProcessKind::Delivery => DocumentType::Delivery,
        ProcessKind::ElectronicDelivery => DocumentType::ElectronicDelivery,
        ProcessKind::ServiceFulfillment => DocumentType::ServiceFulfillment,
        ProcessKind::CustomerAcceptance => DocumentType::CustomerAcceptance,
        ProcessKind::Invoice => DocumentType::Invoice,
        ProcessKind::SalesReturnCase => DocumentType::SalesReturnCase,
        ProcessKind::PurchaseReturnOrder => DocumentType::PurchaseReturnOrder,
    }
}

#[cfg(test)]
mod tests {
    use super::{document_type_of, process_kind_of};
    use bpm::ProcessKind;
    use entities::document_registry::DocumentType;

    const DOCUMENT_TYPES: [DocumentType; 20] = [
        DocumentType::SalesOrder,
        DocumentType::VoucherSalesOrder,
        DocumentType::SalesChangeOrder,
        DocumentType::PurchaseOrder,
        DocumentType::PurchaseChangeOrder,
        DocumentType::StockAdjustment,
        DocumentType::CustomerReceipt,
        DocumentType::SupplierPayment,
        DocumentType::CustomerRefund,
        DocumentType::SupplierRefund,
        DocumentType::ReceiptReversal,
        DocumentType::PaymentReversal,
        DocumentType::PurchaseReceipt,
        DocumentType::Delivery,
        DocumentType::ElectronicDelivery,
        DocumentType::ServiceFulfillment,
        DocumentType::CustomerAcceptance,
        DocumentType::Invoice,
        DocumentType::SalesReturnCase,
        DocumentType::PurchaseReturnOrder,
    ];

    const PROCESS_KINDS: [ProcessKind; 20] = [
        ProcessKind::SalesOrder,
        ProcessKind::VoucherSalesOrder,
        ProcessKind::SalesChangeOrder,
        ProcessKind::PurchaseOrder,
        ProcessKind::PurchaseChangeOrder,
        ProcessKind::StockAdjustment,
        ProcessKind::CustomerReceipt,
        ProcessKind::SupplierPayment,
        ProcessKind::CustomerRefund,
        ProcessKind::SupplierRefund,
        ProcessKind::ReceiptReversal,
        ProcessKind::PaymentReversal,
        ProcessKind::PurchaseReceipt,
        ProcessKind::Delivery,
        ProcessKind::ElectronicDelivery,
        ProcessKind::ServiceFulfillment,
        ProcessKind::CustomerAcceptance,
        ProcessKind::Invoice,
        ProcessKind::SalesReturnCase,
        ProcessKind::PurchaseReturnOrder,
    ];

    /// 20 个单据类型与流程种类双向一一对应，稳定代码一致。
    #[test]
    fn document_type_and_process_kind_are_one_to_one() {
        assert_eq!(DOCUMENT_TYPES.len(), 20);
        assert_eq!(PROCESS_KINDS.len(), 20);

        for document_type in DOCUMENT_TYPES {
            let process_kind = process_kind_of(document_type);
            assert_eq!(document_type_of(process_kind), document_type);
            assert_eq!(process_kind.as_str(), document_type.as_str());
        }

        for process_kind in PROCESS_KINDS {
            let document_type = document_type_of(process_kind);
            assert_eq!(process_kind_of(document_type), process_kind);
        }
    }

    /// 卡券销售单是独立类型，不得并入实物销售单。
    #[test]
    fn voucher_sales_order_is_independent_of_sales_order() {
        assert_ne!(
            process_kind_of(DocumentType::VoucherSalesOrder),
            process_kind_of(DocumentType::SalesOrder)
        );
        assert_eq!(
            document_type_of(ProcessKind::VoucherSalesOrder),
            DocumentType::VoucherSalesOrder
        );
    }
}
