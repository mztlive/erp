//! `DocumentType` 与 `ProcessKind` 的双向穷尽映射入口。
//!
//! 映射规则由 `entities::approval_integration` 独占。本模块保留 BPM 边界脚本
//! 要求的穷尽 match 入口，供尚未迁入实体层的其他域调用点使用。

use bpm::ProcessKind;
use entities::approval_integration;
use entities::document_registry::DocumentType;

/// 将 ERP 单据类型映射为 BPM 流程种类。
///
/// # 参数
/// * `document_type` - 合同固定的 20 种单据类型之一
///
/// # 返回
/// 返回与该单据类型一一对应的流程种类。
///
/// # 错误
/// 不返回错误；穷尽匹配已覆盖全部登记类型。
///
/// # 关键业务约束
/// 转换表由实体层独占；本入口只做穷尽分派，不得另立映射表。
pub fn process_kind_of(document_type: DocumentType) -> ProcessKind {
    match document_type {
        DocumentType::SalesOrder
        | DocumentType::VoucherSalesOrder
        | DocumentType::SalesChangeOrder
        | DocumentType::PurchaseOrder
        | DocumentType::PurchaseChangeOrder
        | DocumentType::StockAdjustment
        | DocumentType::CustomerReceipt
        | DocumentType::SupplierPayment
        | DocumentType::CustomerRefund
        | DocumentType::SupplierRefund
        | DocumentType::ReceiptReversal
        | DocumentType::PaymentReversal
        | DocumentType::PurchaseReceipt
        | DocumentType::Delivery
        | DocumentType::ElectronicDelivery
        | DocumentType::ServiceFulfillment
        | DocumentType::CustomerAcceptance
        | DocumentType::Invoice
        | DocumentType::SalesReturnCase
        | DocumentType::PurchaseReturnOrder => approval_integration::process_kind_of(document_type),
    }
}

/// 将 BPM 流程种类映射回 ERP 单据类型。
///
/// # 参数
/// * `process_kind` - 已冻结的流程种类
///
/// # 返回
/// 返回与该流程种类一一对应的单据类型。
///
/// # 错误
/// 不返回错误；穷尽匹配已覆盖全部冻结种类。
///
/// # 关键业务约束
/// 转换表由实体层独占；本入口只做穷尽分派，不得另立映射表。
pub fn document_type_of(process_kind: ProcessKind) -> DocumentType {
    match process_kind {
        ProcessKind::SalesOrder
        | ProcessKind::VoucherSalesOrder
        | ProcessKind::SalesChangeOrder
        | ProcessKind::PurchaseOrder
        | ProcessKind::PurchaseChangeOrder
        | ProcessKind::StockAdjustment
        | ProcessKind::CustomerReceipt
        | ProcessKind::SupplierPayment
        | ProcessKind::CustomerRefund
        | ProcessKind::SupplierRefund
        | ProcessKind::ReceiptReversal
        | ProcessKind::PaymentReversal
        | ProcessKind::PurchaseReceipt
        | ProcessKind::Delivery
        | ProcessKind::ElectronicDelivery
        | ProcessKind::ServiceFulfillment
        | ProcessKind::CustomerAcceptance
        | ProcessKind::Invoice
        | ProcessKind::SalesReturnCase
        | ProcessKind::PurchaseReturnOrder => approval_integration::document_type_of(process_kind),
    }
}
