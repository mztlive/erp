//! 审批集成身份：单据类型、流程种类与业务对象引用的唯一转换。
//!
//! 本模块是 `DocumentType`、`ProcessKind`、销售 `BusinessType` 与 `SubjectRef`
//! 之间的唯一规则源。BPM 不得反向依赖本 crate。

use crate::document_registry::DocumentType;
use crate::errors::{Error, Result};
use crate::sales_order::BusinessType;
use bpm::{ProcessKind, SubjectRef};

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
/// 不得使用可缺项映射表，也不得接受未登记字符串推导种类。
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
///
/// # 错误
/// 不返回错误；穷尽匹配已覆盖全部冻结种类。
///
/// # 关键业务约束
/// 稳定代码必须与 [`DocumentType::as_str`] 一致，禁止把卡券销售并入实物销售。
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

/// 将销售单业务性质穷尽分派到独立单据类型。
///
/// # 参数
/// * `business_type` - 销售单业务性质
///
/// # 返回
/// `GoodsService` 映射 [`DocumentType::SalesOrder`]，`Voucher` 映射
/// [`DocumentType::VoucherSalesOrder`]。
///
/// # 错误
/// 不返回错误；枚举已穷尽。
///
/// # 关键业务约束
/// 卡券与实物必须使用独立 `DocumentType` / `ProcessKind`，不得二次分流。
pub fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    match business_type {
        BusinessType::GoodsService => DocumentType::SalesOrder,
        BusinessType::Voucher => DocumentType::VoucherSalesOrder,
    }
}

/// 由 BPM 主体种类解析已登记单据类型。
///
/// # 参数
/// * `kind` - BPM 主体持有的稳定种类代码
///
/// # 返回
/// 精确命中登记代码时返回对应单据类型。
///
/// # 错误
/// 空值、未登记种类、大小写变化或前后空白均返回错误，不得回落默认类型。
///
/// # 关键业务约束
/// 精确代码规则由 [`DocumentType::try_from_code`] 拥有，本函数不裁剪、不折叠。
pub fn document_type_from_subject_kind(kind: &str) -> Result<DocumentType> {
    DocumentType::try_from_code(kind)
}

/// 为单据类型与业务主键构造唯一 `SubjectRef`。
///
/// # 参数
/// * `document_type` - 固定单据类型
/// * `business_object_id` - 业务对象主键
///
/// # 返回
/// `subject_kind` 取流程种类稳定码，`subject_id` 取去空白后的业务主键。
///
/// # 错误
/// 主键为空、仅空白或超长时返回错误。
///
/// # 关键业务约束
/// 主体种类必须来自 [`process_kind_of`]，禁止调用方传入任意字符串。
pub fn subject_ref_for(document_type: DocumentType, business_object_id: &str) -> Result<SubjectRef> {
    SubjectRef::new(process_kind_of(document_type).as_str(), business_object_id)
        .map_err(|error| Error::from(error.to_string()))
}

/// 按销售业务性质构造唯一主体引用。
///
/// # 参数
/// * `business_type` - 销售单业务性质
/// * `business_object_id` - 销售单主键
///
/// # 返回
/// 返回对应独立单据类型的主体引用。
///
/// # 错误
/// 主键为空、仅空白或超长时返回错误。
///
/// # 关键业务约束
/// 必须先经 [`document_type_of_sales_business`] 分派，再构造引用。
pub fn subject_ref_for_sales_business(
    business_type: BusinessType,
    business_object_id: &str,
) -> Result<SubjectRef> {
    subject_ref_for(document_type_of_sales_business(business_type), business_object_id)
}

#[cfg(test)]
mod tests {
    use super::{
        document_type_from_subject_kind, document_type_of, document_type_of_sales_business, process_kind_of,
        subject_ref_for, subject_ref_for_sales_business,
    };
    use crate::document_registry::DocumentType;
    use crate::sales_order::BusinessType;
    use bpm::ProcessKind;

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

    /// 20 种单据类型与流程种类双向一一对应，稳定代码一致。
    #[test]
    fn document_type_and_process_kind_round_trip() {
        assert_eq!(DocumentType::ALL.len(), 20);
        assert_eq!(PROCESS_KINDS.len(), 20);

        for document_type in DocumentType::ALL {
            let process_kind = process_kind_of(document_type);
            assert_eq!(document_type_of(process_kind), document_type);
            assert_eq!(process_kind.as_str(), document_type.as_str());
            assert_eq!(
                document_type_from_subject_kind(document_type.as_str()).unwrap(),
                document_type
            );
        }

        for process_kind in PROCESS_KINDS {
            let document_type = document_type_of(process_kind);
            assert_eq!(process_kind_of(document_type), process_kind);
        }
    }

    /// 实物及服务固定映射销售单，卡券固定映射卡券销售单。
    #[test]
    fn sales_business_type_dispatches_to_independent_document_types() {
        assert_eq!(
            document_type_of_sales_business(BusinessType::GoodsService),
            DocumentType::SalesOrder
        );
        assert_eq!(
            document_type_of_sales_business(BusinessType::Voucher),
            DocumentType::VoucherSalesOrder
        );
        assert_ne!(
            process_kind_of(DocumentType::VoucherSalesOrder),
            process_kind_of(DocumentType::SalesOrder)
        );
    }

    /// 非法种类与空主键必须失败关闭，不得回落默认类型。
    #[test]
    fn illegal_kind_and_empty_id_are_rejected() {
        assert!(document_type_from_subject_kind("unknown").is_err());
        assert!(document_type_from_subject_kind("").is_err());
        assert!(document_type_from_subject_kind(" SalesOrder ").is_err());
        assert!(document_type_from_subject_kind("SALES_ORDER").is_err());
        assert!(subject_ref_for(DocumentType::SalesOrder, "").is_err());
        assert!(subject_ref_for(DocumentType::SalesOrder, "   ").is_err());
        assert!(subject_ref_for_sales_business(BusinessType::Voucher, "").is_err());
    }

    /// 每个单据类型都能构造唯一主体引用，销售业务性质不得共用种类。
    #[test]
    fn subject_ref_uses_process_kind_code_and_rejects_collision() {
        let mut kinds = std::collections::BTreeSet::new();
        for document_type in DocumentType::ALL {
            let subject = subject_ref_for(document_type, "doc-1").expect("主体引用必须可构造");
            assert_eq!(subject.subject_id(), "doc-1");
            assert_eq!(subject.subject_kind(), process_kind_of(document_type).as_str());
            assert!(kinds.insert(subject.subject_kind().to_string()));
        }
        assert_eq!(kinds.len(), 20);

        let sales = subject_ref_for_sales_business(BusinessType::GoodsService, "so-1").unwrap();
        let voucher = subject_ref_for_sales_business(BusinessType::Voucher, "so-1").unwrap();
        assert_eq!(sales.subject_kind(), "sales_order");
        assert_eq!(voucher.subject_kind(), "voucher_sales_order");
        assert_ne!(sales.subject_kind(), voucher.subject_kind());
    }
}
