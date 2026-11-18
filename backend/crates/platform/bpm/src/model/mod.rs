//! BPM 流程领域模型。P1 填充定义、实例与执行类型。
//!
//! 本模块只冻结无 ERP 语义的边界类型。模型构造必须由调用方提供时间与 ID，
//! 不得读取系统时钟或自行生成主键。

mod approval_participant;
mod assignment;
pub use assignment::AssigneeResolution;
pub mod command_identity;
pub mod command_receipt;
pub mod instance_assignee;
pub use approval_participant::ApprovalParticipant;
pub mod node_definition;
pub mod node_execution;
pub mod process_definition;
pub mod process_instance;
pub mod transition_definition;
pub mod types;

use chrono::{DateTime, Utc};
pub use command_identity::{
    ApprovalCommandIdentity, CanonicalCommandPayload, CommandDigest, CommandPayloadField, CommandScope,
    IdempotencyKey,
};
pub use command_receipt::ApprovalCommandReceipt;
pub use instance_assignee::ApprovalInstanceAssignee;
pub use node_definition::{ApprovalNodeDefinition, NewNodeDefinition};
pub use node_execution::{ApprovalNodeExecution, NewNodeExecution};
pub use process_definition::ApprovalProcessDefinition;
pub use process_instance::{ApprovalCancellationTaskPolicy, ApprovalProcessInstance, NewProcessInstance};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
pub use transition_definition::ApprovalTransitionDefinition;
pub use types::{
    ApprovalAssigneeBindingSource, ApprovalBlockerCode, ApprovalCommandKind, ApprovalDecision,
    ApprovalDefinitionStatus, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
    ApprovalNodeExecutionStatus, ApprovalNodeType, ApprovalProcessInstanceStatus, ApprovalTerminalResult,
    ApprovalTransitionEvent, ModelError, ModelResult,
};

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
    /// 销项开票申请。
    SalesInvoiceRequest,
    /// 采购付款申请审批。
    PurchasePaymentRequest,
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
    /// 种类与稳定代码的唯一真相源；`as_str`/`try_from_code` 均由此查表。
    const TABLE: [(Self, &'static str); 22] = [
        (Self::SalesOrder, "sales_order"),
        (Self::VoucherSalesOrder, "voucher_sales_order"),
        (Self::SalesChangeOrder, "sales_change_order"),
        (Self::PurchaseOrder, "purchase_order"),
        (Self::PurchaseChangeOrder, "purchase_change_order"),
        (Self::StockAdjustment, "stock_adjustment"),
        (Self::CustomerReceipt, "customer_receipt"),
        (Self::SalesInvoiceRequest, "sales_invoice_request"),
        (Self::PurchasePaymentRequest, "purchase_payment_request"),
        (Self::SupplierPayment, "supplier_payment"),
        (Self::CustomerRefund, "customer_refund"),
        (Self::SupplierRefund, "supplier_refund"),
        (Self::ReceiptReversal, "receipt_reversal"),
        (Self::PaymentReversal, "payment_reversal"),
        (Self::PurchaseReceipt, "purchase_receipt"),
        (Self::Delivery, "delivery"),
        (Self::ElectronicDelivery, "electronic_delivery"),
        (Self::ServiceFulfillment, "service_fulfillment"),
        (Self::CustomerAcceptance, "customer_acceptance"),
        (Self::Invoice, "invoice"),
        (Self::SalesReturnCase, "sales_return_case"),
        (Self::PurchaseReturnOrder, "purchase_return_order"),
    ];

    /// 查询或传递流程配置时使用种类表中冻结的代码，而不是 Rust 变体名；代码非空且不超过 [`Self::MAX_LEN`] 字节。
    ///
    /// # Panics
    /// 枚举新增变体却未登记到种类表时 panic，这是定义不完整的编程错误。
    pub fn as_str(self) -> &'static str {
        Self::TABLE.iter().find(|(kind, _)| *kind == self).map(|(_, code)| *code).expect("种类表覆盖全部变体")
    }

    /// 从外部流程配置解码已登记的稳定代码，按原值精确匹配，不裁剪、不接受别名。
    /// 空值、超长或未登记代码返回 [`Error::InvalidProcessKind`]。
    pub fn try_from_code(code: &str) -> Result<Self> {
        if code.is_empty() || code.len() > Self::MAX_LEN {
            return Err(Error::InvalidProcessKind);
        }

        Self::TABLE
            .iter()
            .find(|(_, known)| *known == code)
            .map(|(kind, _)| *kind)
            .ok_or(Error::InvalidProcessKind)
    }
}

/// 业务对象引用：稳定的 `subject_kind + subject_id`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SubjectRef {
    subject_kind: String,
    subject_id: String,
}

impl SubjectRef {
    /// 启动审批时用种类和主键绑定业务对象，只保留引用，不加载或保存单据快照。两字段都去除首尾空白；
    /// 裁剪后为空或超出 [`SUBJECT_REF_FIELD_MAX_LEN`] 个字节时返回 [`Error::InvalidSubjectRef`]。
    pub fn new(subject_kind: impl Into<String>, subject_id: impl Into<String>) -> Result<Self> {
        let subject_kind = normalize_ref_field(subject_kind, "业务对象种类")?;
        let subject_id = normalize_ref_field(subject_id, "业务对象标识")?;
        Ok(Self { subject_kind, subject_id })
    }

    /// 取冻结的对象种类，以便调用方选择处理该对象引用的业务类型。
    pub fn subject_kind(&self) -> &str {
        &self.subject_kind
    }

    /// 取冻结的对象主键，供调用方在对应种类下定位单据，不需要从实例内读取业务快照。
    pub fn subject_id(&self) -> &str {
        &self.subject_id
    }
}

impl<'de> Deserialize<'de> for SubjectRef {
    // 回读实例的对象引用时复用 `SubjectRef::new`，保证持久化数据也遵守空白和长度约束。
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
    /// 适配层把已确定的处理人标识交给 BPM 时保留其原值，不解释标识格式或裁剪。
    /// 空值或超出 [`PARTICIPANT_ID_MAX_LEN`] 个字节时返回 [`Error::InvalidParticipantId`]。
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

    /// 借用 BPM 保存的处理人标识，供任务映射或身份比对使用，不进行显示名或账号解析。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ParticipantId {
    // 从数据还原处理人引用时复用 `ParticipantId::new`，避免空值或超长身份绕过边界校验。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// BPM 自有 UTC 时间值对象。只接受调用方显式提供的时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// 调用方已经取得 UTC 命令时间时用它构造 BPM 时间戳，确保状态计算不读取系统时钟。
    pub fn from_utc(value: DateTime<Utc>) -> Self {
        Self(value)
    }

    /// 从持久化值或调用方提供的 Unix 秒构造 UTC 时间；超出 `chrono` 表示范围时返回 [`Error::InvalidTimestamp`]。
    pub fn from_unix_secs(unix_secs: i64) -> Result<Self> {
        DateTime::<Utc>::from_timestamp(unix_secs, 0)
            .map(Self)
            .ok_or(Error::InvalidTimestamp("超出可表示范围"))
    }

    /// 为序列化或持久化元数据提取 UTC Unix 秒；它可为负数，模型落盘时再检查能否写入非负秒字段。
    pub fn unix_secs(self) -> i64 {
        self.0.timestamp()
    }
}

impl Serialize for Timestamp {
    // 落盘使用有符号 Unix UTC 秒，保持与已存时间格式一致，不写系统本地时区。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.unix_secs())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    // 回读存储的 Unix 秒，按 `Timestamp::from_unix_secs` 检查可表示范围。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let unix_secs = i64::deserialize(deserializer)?;
        Self::from_unix_secs(unix_secs).map_err(serde::de::Error::custom)
    }
}

// 构造对象引用时统一裁剪种类与主键，并按 UTF-8 字节拒绝空值和过长字段；用 `Error::InvalidSubjectRef` 标明引用无效。
fn normalize_ref_field(value: impl Into<String>, empty_message: &'static str) -> Result<String> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidSubjectRef(empty_message));
    }
    if trimmed.len() > SUBJECT_REF_FIELD_MAX_LEN {
        return Err(Error::InvalidSubjectRef("业务对象引用字段过长"));
    }

    if trimmed.len() == value.len() { Ok(value) } else { Ok(trimmed.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::{
        PARTICIPANT_ID_MAX_LEN, PROCESS_KIND_MAX_LEN, ParticipantId, ProcessKind, SUBJECT_REF_FIELD_MAX_LEN,
        SubjectRef, Timestamp,
    };
    use crate::error::Error;

    // 逐一核对已登记流程种类的稳定代码与长度，保护配置及查询使用的编码合同。
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
            ProcessKind::SalesInvoiceRequest,
            ProcessKind::PurchasePaymentRequest,
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

        assert_eq!(kinds.len(), 22);
        for kind in kinds {
            assert!(!kind.as_str().is_empty());
            assert!(kind.as_str().len() <= PROCESS_KIND_MAX_LEN);
            assert_eq!(ProcessKind::try_from_code(kind.as_str()).unwrap(), kind);
        }
    }

    // 验证未登记或空代码不会被解析成默认流程种类。
    #[test]
    fn process_kind_rejects_unknown_or_empty_code() {
        assert_eq!(ProcessKind::try_from_code(""), Err(Error::InvalidProcessKind));
        assert_eq!(ProcessKind::try_from_code("not_a_registered_kind"), Err(Error::InvalidProcessKind));
        assert_eq!(
            ProcessKind::try_from_code(&"x".repeat(PROCESS_KIND_MAX_LEN + 1)),
            Err(Error::InvalidProcessKind)
        );
    }

    // 分别将业务对象种类或主键设为空或纯空白，确保不会生成不完整的引用。
    #[test]
    fn subject_ref_rejects_empty_fields() {
        assert!(SubjectRef::new(" ", "id-1").is_err());
        assert!(SubjectRef::new("kind", "   ").is_err());

        let subject = SubjectRef::new(" stock_adjustment ", "adj-1").unwrap();
        assert_eq!(subject.subject_kind(), "stock_adjustment");
        assert_eq!(subject.subject_id(), "adj-1");
    }

    // 验证对象引用按裁剪后的长度校验，首尾空白不会掩盖仍超限的字段。
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

    // 用序列化输入覆盖裁剪及空值、超长字段，确保回读与直接构造遵循同样规则。
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

    // 验证缺失身份不会成为可分派审批任务的处理人引用。
    #[test]
    fn participant_id_rejects_empty() {
        assert!(ParticipantId::new("").is_err());
        assert_eq!(ParticipantId::new("user-1").unwrap().as_str(), "user-1");
    }

    // 用 64 字节与更长标识检查处理人身份的接受边界。
    #[test]
    fn participant_id_rejects_overlong() {
        let max = "u".repeat(PARTICIPANT_ID_MAX_LEN);
        let over = "u".repeat(PARTICIPANT_ID_MAX_LEN + 1);
        assert_eq!(ParticipantId::new(&max).unwrap().as_str(), max);
        assert!(ParticipantId::new(&over).is_err());
    }

    // 验证合法处理人能往返序列化，空字符串和超长标识仍不能通过持久化回读。
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

    // 给定固定 Unix 秒，检查 BPM 使用输入时间而非运行时钟。
    #[test]
    fn timestamp_uses_caller_provided_unix_secs() {
        let stamp = Timestamp::from_unix_secs(1_700_000_000).unwrap();
        assert_eq!(stamp.unix_secs(), 1_700_000_000);
        let json = serde_json::to_string(&stamp).unwrap();
        assert_eq!(json, "1700000000");
        let back: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stamp);
    }

    // 验证超出可表示范围的秒值明确报错，不产生截断的审批时间。
    #[test]
    fn timestamp_rejects_unrepresentable_unix_secs() {
        assert_eq!(Timestamp::from_unix_secs(i64::MAX), Err(Error::InvalidTimestamp("超出可表示范围")));
        assert_eq!(Timestamp::from_unix_secs(i64::MIN), Err(Error::InvalidTimestamp("超出可表示范围")));
        assert!(serde_json::from_str::<Timestamp>(&i64::MAX.to_string()).is_err());
    }
}
