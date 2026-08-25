//! BPM 流程领域模型。P1 填充定义、实例与执行类型。
//!
//! 本模块只冻结无 ERP 语义的边界类型。模型构造必须由调用方提供时间与 ID，
//! 不得读取系统时钟或自行生成主键。

pub mod command_receipt;
pub mod instance_assignee;
pub mod node_definition;
pub mod node_execution;
pub mod process_definition;
pub mod process_instance;
pub mod transition_definition;
pub mod types;

pub use command_receipt::ApprovalCommandReceipt;
pub use instance_assignee::ApprovalInstanceAssignee;
pub use node_definition::{ApprovalNodeDefinition, NewNodeDefinition};
pub use node_execution::{ApprovalNodeExecution, NewNodeExecution};
pub use process_definition::ApprovalProcessDefinition;
pub use process_instance::{ApprovalCancellationTaskPolicy, ApprovalProcessInstance, NewProcessInstance};
pub use transition_definition::ApprovalTransitionDefinition;
pub use types::{
    ApprovalAssigneeBindingSource, ApprovalBlockerCode, ApprovalCommandKind, ApprovalDecision,
    ApprovalDefinitionStatus, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
    ApprovalNodeExecutionStatus, ApprovalNodeType, ApprovalProcessInstanceStatus, ApprovalTerminalResult,
    ApprovalTransitionEvent, ModelError, ModelResult,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};

/// 流程种类稳定代码的最大字节长度。
pub const PROCESS_KIND_MAX_LEN: usize = 32;

/// 业务对象引用字段的最大字节长度。
pub const SUBJECT_REF_FIELD_MAX_LEN: usize = 64;

/// 处理人引用的最大字节长度。
pub const PARTICIPANT_ID_MAX_LEN: usize = 64;

/// 流程种类。稳定、非空、有长度上限；不得别名为 ERP 单据类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    /// 实物及服务销售流程。
    SalesOrder,
    /// 卡券销售流程。
    VoucherSalesOrder,
    /// 销售变更流程。
    SalesChangeOrder,
    /// 采购流程。
    PurchaseOrder,
    /// 采购变更流程。
    PurchaseChangeOrder,
    /// 库存调整流程。
    StockAdjustment,
    /// 客户回款流程。
    CustomerReceipt,
    /// 供应商付款流程。
    SupplierPayment,
    /// 客户退款流程。
    CustomerRefund,
    /// 供应商退款流程。
    SupplierRefund,
    /// 回款冲正流程。
    ReceiptReversal,
    /// 付款冲正流程。
    PaymentReversal,
    /// 采购收货流程。
    PurchaseReceipt,
    /// 仓发流程。
    Delivery,
    /// 电子交付流程。
    ElectronicDelivery,
    /// 服务履约流程。
    ServiceFulfillment,
    /// 客户验收流程。
    CustomerAcceptance,
    /// 发票流程。
    Invoice,
    /// 销售退货流程。
    SalesReturnCase,
    /// 采购退货流程。
    PurchaseReturnOrder,
}

impl ProcessKind {
    /// 稳定代码的最大字节长度。
    pub const MAX_LEN: usize = PROCESS_KIND_MAX_LEN;

    /// 返回流程种类的稳定代码。
    ///
    /// # 返回
    /// 返回非空且不超过 [`Self::MAX_LEN`] 的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SalesOrder => "sales_order",
            Self::VoucherSalesOrder => "voucher_sales_order",
            Self::SalesChangeOrder => "sales_change_order",
            Self::PurchaseOrder => "purchase_order",
            Self::PurchaseChangeOrder => "purchase_change_order",
            Self::StockAdjustment => "stock_adjustment",
            Self::CustomerReceipt => "customer_receipt",
            Self::SupplierPayment => "supplier_payment",
            Self::CustomerRefund => "customer_refund",
            Self::SupplierRefund => "supplier_refund",
            Self::ReceiptReversal => "receipt_reversal",
            Self::PaymentReversal => "payment_reversal",
            Self::PurchaseReceipt => "purchase_receipt",
            Self::Delivery => "delivery",
            Self::ElectronicDelivery => "electronic_delivery",
            Self::ServiceFulfillment => "service_fulfillment",
            Self::CustomerAcceptance => "customer_acceptance",
            Self::Invoice => "invoice",
            Self::SalesReturnCase => "sales_return_case",
            Self::PurchaseReturnOrder => "purchase_return_order",
        }
    }

    /// 仅接受已冻结的稳定代码。
    ///
    /// # 参数
    /// * `code` - 流程种类稳定代码
    ///
    /// # 返回
    /// 代码属于已冻结集合时返回对应种类。
    ///
    /// # 错误
    /// 空值、超长或未登记代码返回 [`Error::InvalidProcessKind`]。
    pub fn try_from_code(code: &str) -> Result<Self> {
        if code.is_empty() || code.len() > Self::MAX_LEN {
            return Err(Error::InvalidProcessKind);
        }
        match code {
            "sales_order" => Ok(Self::SalesOrder),
            "voucher_sales_order" => Ok(Self::VoucherSalesOrder),
            "sales_change_order" => Ok(Self::SalesChangeOrder),
            "purchase_order" => Ok(Self::PurchaseOrder),
            "purchase_change_order" => Ok(Self::PurchaseChangeOrder),
            "stock_adjustment" => Ok(Self::StockAdjustment),
            "customer_receipt" => Ok(Self::CustomerReceipt),
            "supplier_payment" => Ok(Self::SupplierPayment),
            "customer_refund" => Ok(Self::CustomerRefund),
            "supplier_refund" => Ok(Self::SupplierRefund),
            "receipt_reversal" => Ok(Self::ReceiptReversal),
            "payment_reversal" => Ok(Self::PaymentReversal),
            "purchase_receipt" => Ok(Self::PurchaseReceipt),
            "delivery" => Ok(Self::Delivery),
            "electronic_delivery" => Ok(Self::ElectronicDelivery),
            "service_fulfillment" => Ok(Self::ServiceFulfillment),
            "customer_acceptance" => Ok(Self::CustomerAcceptance),
            "invoice" => Ok(Self::Invoice),
            "sales_return_case" => Ok(Self::SalesReturnCase),
            "purchase_return_order" => Ok(Self::PurchaseReturnOrder),
            _ => Err(Error::InvalidProcessKind),
        }
    }
}

/// 业务对象引用：稳定的 `subject_kind + subject_id`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SubjectRef {
    subject_kind: String,
    subject_id: String,
}

impl SubjectRef {
    /// 由调用方提供的稳定 kind/id 构造业务对象引用。
    ///
    /// # 参数
    /// * `subject_kind` - 对象种类
    /// * `subject_id` - 对象主键
    ///
    /// # 返回
    /// 两个字段均非空且未超长时返回引用。
    ///
    /// # 错误
    /// 空值或超出 [`SUBJECT_REF_FIELD_MAX_LEN`] 时返回 [`Error::InvalidSubjectRef`]。
    pub fn new(subject_kind: impl Into<String>, subject_id: impl Into<String>) -> Result<Self> {
        let subject_kind = normalize_ref_field(subject_kind, "业务对象种类")?;
        let subject_id = normalize_ref_field(subject_id, "业务对象标识")?;
        Ok(Self {
            subject_kind,
            subject_id,
        })
    }

    /// 返回对象种类。
    ///
    /// # 返回
    /// 返回构造时写入的稳定种类。
    pub fn subject_kind(&self) -> &str {
        &self.subject_kind
    }

    /// 返回对象主键。
    ///
    /// # 返回
    /// 返回构造时写入的稳定标识。
    pub fn subject_id(&self) -> &str {
        &self.subject_id
    }
}

impl<'de> Deserialize<'de> for SubjectRef {
    /// 反序列化后走 [`SubjectRef::new`]，空值与超长字段不得绕过构造校验。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct RawSubjectRef {
            subject_kind: String,
            subject_id: String,
        }

        let raw = RawSubjectRef::deserialize(deserializer)?;
        Self::new(raw.subject_kind, raw.subject_id).map_err(serde::de::Error::custom)
    }
}

/// BPM 对处理人的不透明引用。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ParticipantId(String);

impl ParticipantId {
    /// 由调用方提供的不透明处理人标识构造引用。
    ///
    /// # 参数
    /// * `value` - 处理人标识
    ///
    /// # 返回
    /// 非空且未超长时返回引用。
    ///
    /// # 错误
    /// 空值或超出 [`PARTICIPANT_ID_MAX_LEN`] 时返回 [`Error::InvalidParticipantId`]。
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::InvalidParticipantId("处理人引用不能为空"));
        }
        if value.len() > PARTICIPANT_ID_MAX_LEN {
            return Err(Error::InvalidParticipantId("处理人引用过长"));
        }
        Ok(Self(value))
    }

    /// 返回不透明处理人标识。
    ///
    /// # 返回
    /// 返回构造时写入的字符串。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ParticipantId {
    /// 反序列化后走 [`ParticipantId::new`]，空值与超长标识不得绕过构造校验。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// BPM 自有 UTC 时间值对象。只接受调用方显式提供的时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// 由调用方提供的 UTC 时间构造时间戳。
    ///
    /// # 参数
    /// * `value` - 调用方已经取得的 UTC 时间
    ///
    /// # 返回
    /// 返回 BPM 时间值对象。
    pub fn from_utc(value: DateTime<Utc>) -> Self {
        Self(value)
    }

    /// 由调用方提供的 UTC 秒级时间戳构造。
    ///
    /// # 参数
    /// * `unix_secs` - UTC 秒
    ///
    /// # 返回
    /// 时间戳可表示时返回值对象。
    ///
    /// # 错误
    /// 超出 `chrono` 可表示范围时返回 [`Error::InvalidTimestamp`]。
    pub fn from_unix_secs(unix_secs: i64) -> Result<Self> {
        DateTime::<Utc>::from_timestamp(unix_secs, 0)
            .map(Self)
            .ok_or(Error::InvalidTimestamp("超出可表示范围"))
    }

    /// 返回 UTC 秒级时间戳。
    ///
    /// # 返回
    /// 返回秒级整数。
    pub fn unix_secs(self) -> i64 {
        self.0.timestamp()
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.unix_secs())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let unix_secs = i64::deserialize(deserializer)?;
        Self::from_unix_secs(unix_secs).map_err(serde::de::Error::custom)
    }
}

/// 规范化引用字段：去首尾空白、拒绝空值与超长。
///
/// # 参数
/// * `value` - 原始字段
/// * `empty_message` - 空值错误说明
///
/// # 错误
/// 空值或超出长度上限时返回 [`Error::InvalidSubjectRef`]。
fn normalize_ref_field(value: impl Into<String>, empty_message: &'static str) -> Result<String> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidSubjectRef(empty_message));
    }
    if trimmed.len() > SUBJECT_REF_FIELD_MAX_LEN {
        return Err(Error::InvalidSubjectRef("业务对象引用字段过长"));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        ParticipantId, ProcessKind, SubjectRef, Timestamp, PARTICIPANT_ID_MAX_LEN, PROCESS_KIND_MAX_LEN,
        SUBJECT_REF_FIELD_MAX_LEN,
    };
    use crate::error::Error;

    /// 已冻结种类的稳定代码非空且不超过长度上限。
    #[test]
    fn process_kind_codes_are_stable_and_bounded() {
        let kinds = [
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
        assert_eq!(kinds.len(), 20);
        for kind in kinds {
            assert!(!kind.as_str().is_empty());
            assert!(kind.as_str().len() <= PROCESS_KIND_MAX_LEN);
            assert_eq!(ProcessKind::try_from_code(kind.as_str()).unwrap(), kind);
        }
    }

    /// 未登记代码不得构造流程种类。
    #[test]
    fn process_kind_rejects_unknown_or_empty_code() {
        assert_eq!(ProcessKind::try_from_code(""), Err(Error::InvalidProcessKind));
        assert_eq!(
            ProcessKind::try_from_code("not_a_registered_kind"),
            Err(Error::InvalidProcessKind)
        );
        assert_eq!(
            ProcessKind::try_from_code(&"x".repeat(PROCESS_KIND_MAX_LEN + 1)),
            Err(Error::InvalidProcessKind)
        );
    }

    /// 业务对象引用拒绝空字段。
    #[test]
    fn subject_ref_rejects_empty_fields() {
        assert!(SubjectRef::new(" ", "id-1").is_err());
        assert!(SubjectRef::new("kind", "   ").is_err());
        let subject = SubjectRef::new(" stock_adjustment ", "adj-1").unwrap();
        assert_eq!(subject.subject_kind(), "stock_adjustment");
        assert_eq!(subject.subject_id(), "adj-1");
    }

    /// 业务对象引用拒绝超长字段，含 trim 后仍超长。
    #[test]
    fn subject_ref_rejects_overlong_fields() {
        let max = "k".repeat(SUBJECT_REF_FIELD_MAX_LEN);
        let over = "k".repeat(SUBJECT_REF_FIELD_MAX_LEN + 1);
        assert!(SubjectRef::new(&max, "id-1").is_ok());
        assert!(SubjectRef::new("kind", &max).is_ok());
        assert!(SubjectRef::new(&over, "id-1").is_err());
        assert!(SubjectRef::new("kind", &over).is_err());
        assert!(SubjectRef::new(format!(" {over} "), "id-1").is_err());
        let trimmed_max = SubjectRef::new(format!(" {max} "), "id-1").unwrap();
        assert_eq!(trimmed_max.subject_kind(), max);
    }

    /// 业务对象引用反序列化必须走 `new()`，空值与超长不得绕过。
    #[test]
    fn subject_ref_deserialize_uses_constructor() {
        let subject = SubjectRef::new("stock_adjustment", "adj-1").unwrap();
        let json = serde_json::to_string(&subject).unwrap();
        let back: SubjectRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, subject);

        assert!(serde_json::from_str::<SubjectRef>(r#"{"subject_kind":"","subject_id":"adj-1"}"#).is_err());
        assert!(serde_json::from_str::<SubjectRef>(r#"{"subject_kind":"kind","subject_id":""}"#).is_err());
        assert!(serde_json::from_str::<SubjectRef>(r#"{"subject_kind":"  ","subject_id":"adj-1"}"#).is_err());
        let over = "k".repeat(SUBJECT_REF_FIELD_MAX_LEN + 1);
        let over_json = format!(r#"{{"subject_kind":"{over}","subject_id":"id"}}"#);
        assert!(serde_json::from_str::<SubjectRef>(&over_json).is_err());

        let trimmed: SubjectRef =
            serde_json::from_str(r#"{"subject_kind":" stock_adjustment ","subject_id":"adj-1"}"#).unwrap();
        assert_eq!(trimmed, subject);
    }

    /// 处理人引用拒绝空值。
    #[test]
    fn participant_id_rejects_empty() {
        assert!(ParticipantId::new("").is_err());
        assert_eq!(ParticipantId::new("user-1").unwrap().as_str(), "user-1");
    }

    /// 处理人引用拒绝超长值，64 字节边界可构造。
    #[test]
    fn participant_id_rejects_overlong() {
        let max = "u".repeat(PARTICIPANT_ID_MAX_LEN);
        let over = "u".repeat(PARTICIPANT_ID_MAX_LEN + 1);
        assert_eq!(ParticipantId::new(&max).unwrap().as_str(), max);
        assert!(ParticipantId::new(&over).is_err());
    }

    /// 处理人引用反序列化必须走 `new()`，空字符串与超长不得绕过。
    #[test]
    fn participant_id_deserialize_uses_constructor() {
        let participant = ParticipantId::new("user-1").unwrap();
        let json = serde_json::to_string(&participant).unwrap();
        assert_eq!(json, "\"user-1\"");
        let back: ParticipantId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, participant);

        assert!(serde_json::from_str::<ParticipantId>("\"\"").is_err());
        let over = "u".repeat(PARTICIPANT_ID_MAX_LEN + 1);
        assert!(serde_json::from_str::<ParticipantId>(&format!("\"{over}\"")).is_err());
    }

    /// 时间戳只接受调用方提供的秒值。
    #[test]
    fn timestamp_uses_caller_provided_unix_secs() {
        let stamp = Timestamp::from_unix_secs(1_700_000_000).unwrap();
        assert_eq!(stamp.unix_secs(), 1_700_000_000);
        let json = serde_json::to_string(&stamp).unwrap();
        assert_eq!(json, "1700000000");
        let back: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stamp);
    }

    /// 超出 chrono 可表示范围的秒值必须失败关闭。
    #[test]
    fn timestamp_rejects_unrepresentable_unix_secs() {
        assert_eq!(
            Timestamp::from_unix_secs(i64::MAX),
            Err(Error::InvalidTimestamp("超出可表示范围"))
        );
        assert_eq!(
            Timestamp::from_unix_secs(i64::MIN),
            Err(Error::InvalidTimestamp("超出可表示范围"))
        );
        assert!(serde_json::from_str::<Timestamp>(&i64::MAX.to_string()).is_err());
    }
}
