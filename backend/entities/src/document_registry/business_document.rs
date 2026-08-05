//! `business_document`：跨域单据稳定注册表（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::BusinessDocumentId;
use crate::validation::normalize_required_text;

/// 单据编号最大长度。
const DOCUMENT_NO_MAX_LEN: usize = 128;

/// 强类型业务表类型（数据模型 §6.1 `business_document.document_type`）。
///
/// 只收录一期（§5.3）形成正式事实、需要全局编号搜索与跨域关联的强类型单据表；
/// 采购二次确认等「行为不产生单据」的对象不在此列。二期新增单据类型属于
/// 地基修订候选（需更新本枚举与注册表校验）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    /// 销售单。
    SalesOrder,
    /// 销售变更单。
    SalesChangeOrder,
    /// 采购单。
    PurchaseOrder,
    /// 采购变更单。
    PurchaseChangeOrder,
    /// 采购收货单。
    PurchaseReceipt,
    /// 仓发单。
    Delivery,
    /// 电子交付单。
    ElectronicDelivery,
    /// 服务履约单。
    ServiceFulfillment,
    /// 客户验收单。
    CustomerAcceptance,
    /// 库存调整单。
    StockAdjustment,
    /// 客户回款单。
    CustomerReceipt,
    /// 供应商付款单。
    SupplierPayment,
    /// 发票。
    Invoice,
    /// 销售退货单。
    SalesReturnCase,
    /// 采购退货单。
    PurchaseReturnOrder,
    /// 客户退款单。
    CustomerRefund,
    /// 供应商退款单。
    SupplierRefund,
    /// 回款冲正单。
    ReceiptReversal,
    /// 付款冲正单。
    PaymentReversal,
}

impl DocumentType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::SalesOrder => "销售单",
            Self::SalesChangeOrder => "销售变更单",
            Self::PurchaseOrder => "采购单",
            Self::PurchaseChangeOrder => "采购变更单",
            Self::PurchaseReceipt => "采购收货单",
            Self::Delivery => "仓发单",
            Self::ElectronicDelivery => "电子交付单",
            Self::ServiceFulfillment => "服务履约单",
            Self::CustomerAcceptance => "客户验收单",
            Self::StockAdjustment => "库存调整单",
            Self::CustomerReceipt => "客户回款单",
            Self::SupplierPayment => "供应商付款单",
            Self::Invoice => "发票",
            Self::SalesReturnCase => "销售退货单",
            Self::PurchaseReturnOrder => "采购退货单",
            Self::CustomerRefund => "客户退款单",
            Self::SupplierRefund => "供应商退款单",
            Self::ReceiptReversal => "回款冲正单",
            Self::PaymentReversal => "付款冲正单",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SalesOrder => "sales_order",
            Self::SalesChangeOrder => "sales_change_order",
            Self::PurchaseOrder => "purchase_order",
            Self::PurchaseChangeOrder => "purchase_change_order",
            Self::PurchaseReceipt => "purchase_receipt",
            Self::Delivery => "delivery",
            Self::ElectronicDelivery => "electronic_delivery",
            Self::ServiceFulfillment => "service_fulfillment",
            Self::CustomerAcceptance => "customer_acceptance",
            Self::StockAdjustment => "stock_adjustment",
            Self::CustomerReceipt => "customer_receipt",
            Self::SupplierPayment => "supplier_payment",
            Self::Invoice => "invoice",
            Self::SalesReturnCase => "sales_return_case",
            Self::PurchaseReturnOrder => "purchase_return_order",
            Self::CustomerRefund => "customer_refund",
            Self::SupplierRefund => "supplier_refund",
            Self::ReceiptReversal => "receipt_reversal",
            Self::PaymentReversal => "payment_reversal",
        }
    }
}

/// 单据注册创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessDocumentData {
    /// 强类型业务表类型。
    pub document_type: DocumentType,
    /// 全局可查询业务编号。
    pub document_no: String,
}

/// 跨域单据稳定注册表实体（数据模型 §6.1）。
///
/// 注册表只保存类型和编号，不承载业务字段（§5.1）；`(document_type,
/// document_no)` 唯一约束与 `document_no` 全局搜索索引由 P2 建立，
/// 与强类型业务表的一对一注册校验由 P3 事务完成（§6.1）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct BusinessDocument {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 强类型业务表类型。
    pub document_type: DocumentType,
    /// 全局可查询业务编号。
    pub document_no: String,
    /// 首次成为正式事实的时间（创建时为空，由强类型单据正式化时写入）。
    pub formalized_at: Option<Instant>,
}

impl BusinessDocument {
    /// 创建单据注册。
    ///
    /// 完成 document_no 的校验与规范化（去首尾空白、非空、长度上限）；
    /// 注册记录由强类型业务表所在事务一并创建，不允许脱离业务表单独创建
    /// 「空单据」（§6.1，由 P3 校验注册记录与业务表一对一）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::BusinessDocumentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的注册实体（`formalized_at` 为 `None`）。
    ///
    /// # 错误
    /// 当 document_no 为空或超长时返回错误。
    pub fn new(id: BusinessDocumentId, data: BusinessDocumentData) -> Result<Self> {
        let document_no = normalize_required_text(
            data.document_no,
            "单据编号不能为空",
            DOCUMENT_NO_MAX_LEN,
            "单据编号过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            document_type: data.document_type,
            document_no,
            formalized_at: None,
        })
    }

    /// 标记单据首次正式化。
    ///
    /// `formalized_at` 只记录首次成为正式事实的时间，重复调用不覆盖。
    ///
    /// # 参数
    /// * `at` - 正式化时刻
    ///
    /// # 返回
    /// 无返回值；首次调用写入 `formalized_at`，后续调用保持原值。
    pub fn formalize(&mut self, at: Instant) {
        if self.formalized_at.is_none() {
            self.formalized_at = Some(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BusinessDocument, BusinessDocumentData, DocumentType};
    use crate::common::time::Instant;
    use crate::ids::BusinessDocumentId;
    use serde_json;

    fn data() -> BusinessDocumentData {
        BusinessDocumentData {
            document_type: DocumentType::SalesOrder,
            document_no: " SO-2025-001 ".to_string(),
        }
    }

    /// happy path：编号去首尾空白，类型与编号正确落库，初始未正式化。
    #[test]
    fn new_trims_document_no_and_starts_unformalized() {
        let doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        assert_eq!(doc.document_no, "SO-2025-001");
        assert_eq!(doc.document_type, DocumentType::SalesOrder);
        assert!(doc.formalized_at.is_none());
    }

    /// 失败路径：空编号被拒。
    #[test]
    fn new_rejects_empty_document_no() {
        let payload = BusinessDocumentData {
            document_no: "   ".to_string(),
            ..data()
        };
        assert!(BusinessDocument::new(BusinessDocumentId::new("bd-1"), payload).is_err());
    }

    /// 失败路径：超长编号被拒。
    #[test]
    fn new_rejects_overlong_document_no() {
        let payload = BusinessDocumentData {
            document_no: "x".repeat(129),
            ..data()
        };
        assert!(BusinessDocument::new(BusinessDocumentId::new("bd-1"), payload).is_err());
    }

    /// 正式化时间只记录首次。
    #[test]
    fn formalize_only_records_first_time() {
        let mut doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        let first = Instant::from_unix_secs(1_700_000_000);
        let second = Instant::from_unix_secs(1_700_086_400);
        doc.formalize(first);
        doc.formalize(second);
        assert_eq!(doc.formalized_at.unwrap(), first);
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn document_type_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&DocumentType::SalesChangeOrder).unwrap(),
            "\"sales_change_order\""
        );
        assert_eq!(DocumentType::ReceiptReversal.as_str(), "receipt_reversal");
        assert_eq!(DocumentType::Invoice.label(), "发票");
        assert_eq!(DocumentType::PaymentReversal.label(), "付款冲正单");
    }

    /// BSON 往返（实体层持久化形态与 P0 约定一致）。
    #[test]
    fn entity_roundtrips_through_bson() {
        let doc = BusinessDocument::new(BusinessDocumentId::new("bd-1"), data()).unwrap();
        let roundtrip: BusinessDocument = bson::from_document(bson::to_document(&doc).unwrap()).unwrap();
        assert_eq!(roundtrip, doc);
    }
}
