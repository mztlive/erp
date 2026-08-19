//! `electronic_delivery`：电子交付记录（数据模型 §6.7）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。
//!
//! 公共关键字段按 §6.7 字典；字典含 `occurred_at` 等正式事实字段，按 §4.3
//! 组合 `FactBase`。敏感交付对象（`recipient_snapshot`）按 §4.5.5 建模为
//! **加密快照 + 带密钥 HMAC 查询指纹**两个字段，自定义 `Debug` 不泄漏。
//! 状态机按 §7.5：草稿 → 已确认 → 已冲正（`CONFIRMED` 后不可覆盖，失败后
//! 重做形成新记录，§6.7）。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::fact::FactBase;
use crate::common::source::SourceType;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    ElectronicDeliveryId, FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId,
};
use crate::money::Quantity;
use crate::validation::normalize_required_text;

use super::fingerprint::{hmac_sha256_hex, validate_fingerprint, FINGERPRINT_HEX_LEN};

/// 履约记录号最大长度。
const FULFILLMENT_NO_MAX_LEN: usize = 64;
/// 交付对象加密快照最大长度。
const RECIPIENT_SNAPSHOT_MAX_LEN: usize = 4096;
/// 记录人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;
/// 来源引用最大长度。
const SOURCE_REFERENCE_MAX_LEN: usize = 256;

/// 电子交付状态（数据模型 §6.7：草稿、已确认、已冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ElectronicDeliveryState {
    /// 草稿。
    Draft,
    /// 已确认（不可覆盖，纠错只能冲正）。
    Confirmed,
    /// 已冲正（不可逆终态）。
    Reversed,
}

impl ElectronicDeliveryState {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Confirmed => "已确认",
            Self::Reversed => "已冲正",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Confirmed => "CONFIRMED",
            Self::Reversed => "REVERSED",
        }
    }

    /// 判断是否可编辑（仅草稿）。
    ///
    /// # 返回
    /// 草稿状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        matches!(self, Self::Draft)
    }
}

impl DocumentState for ElectronicDeliveryState {
    /// 固定邻接矩阵（§7.5 定向链，`REVERSED` 为不可逆终态）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Confirmed],
            Self::Confirmed => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 履约结果（数据模型 §6.7：成功、部分成功、失败）。
///
/// 与 [`crate::fulfillment::service_fulfillment::ServiceFulfillment`] 共用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentResult {
    /// 成功。
    Success,
    /// 部分成功。
    PartialSuccess,
    /// 失败（失败后重做形成新记录，不覆盖原记录）。
    Failure,
}

impl FulfillmentResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "成功",
            Self::PartialSuccess => "部分成功",
            Self::Failure => "失败",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::PartialSuccess => "PARTIAL_SUCCESS",
            Self::Failure => "FAILURE",
        }
    }
}

/// 电子交付记录创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ElectronicDeliveryData {
    /// 履约记录号（全局唯一）。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 对应采购行到销售行的明确分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 必要交付对象的加密快照（P3 加密生成；本层只定义结构）。
    pub recipient_snapshot: String,
    /// 交付对象快照带密钥 HMAC 查询指纹（64 位十六进制）。
    pub recipient_snapshot_fingerprint: String,
    /// 交付数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 聚合内稳定序号（正式事实，§4.3）。
    pub fact_no: String,
    /// 实际交付时间。
    pub occurred_at: Instant,
    /// ERP 记录时间。
    pub recorded_at: Instant,
    /// ERP 记录人或系统身份。
    pub recorded_by: String,
    /// 事实来源类型。
    pub source_type: SourceType,
    /// 可追溯的来源单据或消息引用。
    pub source_reference: Option<String>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明文本。
    pub reason_text: Option<String>,
}

/// 电子交付记录更新数据（仅草稿可更新）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ElectronicDeliveryUpdate {
    /// 履约结果；`None` 表示不修改。
    pub result: Option<FulfillmentResult>,
    /// 业务凭证；`None` 表示不修改。
    pub evidence_attachment_id: Option<Option<FileAssetId>>,
}

/// 电子交付记录实体（数据模型 §6.7）。
///
/// 组合 `FactBase` 表达正式事实语义（§4.3）；已确认记录不可覆盖，失败后重做
/// 形成新记录；冲正由反向事实表达（§6.7）。已确认/已冲正不设业务软删除。
/// `Debug` 输出不泄漏交付对象快照及其指纹。
#[derive(Serialize, Deserialize, Clone, Entity)]
pub struct ElectronicDelivery {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub fact: FactBase,
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 对应采购行到销售行的明确分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 必要交付对象的加密快照。
    pub recipient_snapshot: String,
    /// 交付对象快照带密钥 HMAC 查询指纹。
    pub recipient_snapshot_fingerprint: String,
    /// 交付数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 当前状态。
    pub status: ElectronicDeliveryState,
}

impl fmt::Debug for ElectronicDelivery {
    /// 脱敏调试输出：交付对象快照与指纹一律以 `<redacted>` 呈现。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ElectronicDelivery")
            .field("base", &self.base)
            .field("fact_no", &self.fact.fact_no)
            .field("occurred_at", &self.fact.occurred_at)
            .field("recorded_at", &self.fact.recorded_at)
            .field("recorded_by", &self.fact.recorded_by)
            .field("source_type", &self.fact.source_type)
            .field("source_reference", &self.fact.source_reference)
            .field("reason_code", &self.fact.reason_code)
            .field("reason_text", &self.fact.reason_text)
            .field("fulfillment_no", &self.fulfillment_no)
            .field("sales_order_line_id", &self.sales_order_line_id)
            .field("purchase_order_id", &self.purchase_order_id)
            .field(
                "purchase_line_sales_allocation_id",
                &self.purchase_line_sales_allocation_id,
            )
            .field("recipient_snapshot", &"<redacted>")
            .field("recipient_snapshot_fingerprint", &"<redacted>")
            .field("quantity", &self.quantity)
            .field("result", &self.result)
            .field("evidence_attachment_id", &self.evidence_attachment_id)
            .field("status", &self.status)
            .finish()
    }
}

impl PartialEq for ElectronicDelivery {
    /// 全字段语义相等（`Debug` 脱敏不影响比较）。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.fact.fact_no == other.fact.fact_no
            && self.fact.occurred_at == other.fact.occurred_at
            && self.fact.recorded_at == other.fact.recorded_at
            && self.fact.recorded_by == other.fact.recorded_by
            && self.fact.source_type == other.fact.source_type
            && self.fact.source_reference == other.fact.source_reference
            && self.fact.reason_code == other.fact.reason_code
            && self.fact.reason_text == other.fact.reason_text
            && self.fulfillment_no == other.fulfillment_no
            && self.sales_order_line_id == other.sales_order_line_id
            && self.purchase_order_id == other.purchase_order_id
            && self.purchase_line_sales_allocation_id == other.purchase_line_sales_allocation_id
            && self.recipient_snapshot == other.recipient_snapshot
            && self.recipient_snapshot_fingerprint == other.recipient_snapshot_fingerprint
            && self.quantity == other.quantity
            && self.result == other.result
            && self.evidence_attachment_id == other.evidence_attachment_id
            && self.status == other.status
    }
}

impl Eq for ElectronicDelivery {}

impl ElectronicDelivery {
    /// 生成交付对象快照查询指纹。
    ///
    /// 对必要交付对象规范化原文字符串计算带密钥 HMAC-SHA256（§4.5.5，
    /// 禁止裸摘要）；密钥不持久化，精确查询时用同一密钥比对指纹。
    ///
    /// # 参数
    /// * `plain` - 交付对象原文字符串
    /// * `key` - 查询指纹密钥字节
    ///
    /// # 返回
    /// 返回 64 位小写十六进制指纹。
    pub fn recipient_snapshot_fingerprint(plain: &str, key: &[u8]) -> String {
        hmac_sha256_hex(key, plain.as_bytes())
    }

    /// 创建电子交付记录（初始状态为草稿）。
    ///
    /// 完成履约记录号、记录人、来源引用的规范化与快照指纹格式校验；校验
    /// `recorded_at` 不早于 `occurred_at`。已确认记录必须引用同一销售明细和
    /// 采购单的有效采购销售分配——跨聚合校验由 P3 完成（§6.7）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ElectronicDeliveryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的电子交付记录实体。
    ///
    /// # 错误
    /// 必填字段为空/超长、快照指纹格式非法、交付数量非正或记录时间早于
    /// 发生时间时返回错误。
    pub fn new(id: ElectronicDeliveryId, data: ElectronicDeliveryData) -> Result<Self> {
        let fulfillment_no = normalize_required_text(
            data.fulfillment_no,
            "履约记录号不能为空",
            FULFILLMENT_NO_MAX_LEN,
            "履约记录号过长",
        )?;
        let recipient_snapshot = normalize_required_text(
            data.recipient_snapshot,
            "交付对象加密快照不能为空",
            RECIPIENT_SNAPSHOT_MAX_LEN,
            "交付对象加密快照过长",
        )?;
        let recipient_snapshot_fingerprint = normalize_required_text(
            data.recipient_snapshot_fingerprint,
            "交付对象查询指纹不能为空",
            FINGERPRINT_HEX_LEN,
            "交付对象查询指纹过长",
        )?;
        validate_fingerprint(&recipient_snapshot_fingerprint)?;
        let recorded_by =
            normalize_required_text(data.recorded_by, "记录人不能为空", ACTOR_MAX_LEN, "记录人过长")?;
        let source_reference = normalize_optional_source_reference(data.source_reference);
        if data.recorded_at < data.occurred_at {
            return Err(Error::from("记录时间不得早于实际交付时间"));
        }
        if data.quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("交付数量必须为正数"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            fact: FactBase::new(
                data.fact_no,
                data.occurred_at,
                data.recorded_at,
                recorded_by,
                data.source_type,
                source_reference,
                data.reason_code,
                data.reason_text,
            ),
            fulfillment_no,
            sales_order_line_id: data.sales_order_line_id,
            purchase_order_id: data.purchase_order_id,
            purchase_line_sales_allocation_id: data.purchase_line_sales_allocation_id,
            recipient_snapshot,
            recipient_snapshot_fingerprint,
            quantity: data.quantity,
            result: data.result,
            evidence_attachment_id: data.evidence_attachment_id,
            status: ElectronicDeliveryState::Draft,
        })
    }

    /// 更新电子交付记录（仅草稿）。
    ///
    /// 已确认记录不可覆盖（§6.7）；确认后的纠错只能冲正并新建记录。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不可编辑时返回错误。
    pub fn update(&mut self, update: ElectronicDeliveryUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(result) = update.result {
            self.result = result;
        }
        if let Some(evidence_attachment_id) = update.evidence_attachment_id {
            self.evidence_attachment_id = evidence_attachment_id;
        }
        Ok(())
    }

    /// 确认电子交付（草稿 → 已确认）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非草稿）时返回错误。
    pub fn confirm(&mut self) -> Result<()> {
        ensure_transition(self.status, ElectronicDeliveryState::Confirmed)?;
        self.status = ElectronicDeliveryState::Confirmed;
        Ok(())
    }

    /// 冲正电子交付（已确认 → 已冲正，终态）。
    ///
    /// `REVERSED` 表示存在正式反向事实，不删除原记录（§4.5.1、§7.5）；
    /// 反向事实由 P3 形成。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（草稿或已冲正）时返回错误。
    pub fn reverse(&mut self) -> Result<()> {
        ensure_transition(self.status, ElectronicDeliveryState::Reversed)?;
        self.status = ElectronicDeliveryState::Reversed;
        Ok(())
    }

    /// 判断当前状态是否可编辑。
    ///
    /// # 返回
    /// 草稿状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        self.status.is_editable()
    }

    /// 校验当前状态可编辑。
    ///
    /// # 返回
    /// 可编辑返回 `Ok(())`。
    ///
    /// # 错误
    /// 已确认/已冲正的记录不可编辑时返回错误。
    fn ensure_editable(&self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("已确认或已冲正的电子交付记录不可覆盖"));
        }
        Ok(())
    }
}

/// 规范化可选来源引用。
///
/// # 参数
/// * `source_reference` - 来源单据或消息引用
///
/// # 返回
/// 返回去除首尾空白并截断到长度上限后的引用。
fn normalize_optional_source_reference(source_reference: Option<String>) -> Option<String> {
    source_reference
        .map(|value| value.trim().chars().take(SOURCE_REFERENCE_MAX_LEN).collect())
        .filter(|value: &String| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ElectronicDeliveryId;
    use std::str::FromStr;

    const PLAINTEXT_RECIPIENT: &str = "收货人 李四 13812345678 电子邮箱 lisi@example.com";
    const FINGERPRINT_KEY: &[u8] = b"test-fingerprint-key";

    fn data() -> ElectronicDeliveryData {
        ElectronicDeliveryData {
            fulfillment_no: " ED-2026-001 ".to_string(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            recipient_snapshot: "ciphertext-recipient...".to_string(),
            recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                PLAINTEXT_RECIPIENT,
                FINGERPRINT_KEY,
            ),
            quantity: Quantity::from_str("2").unwrap(),
            result: FulfillmentResult::Success,
            evidence_attachment_id: Some(FileAssetId::new("file-1")),
            fact_no: "F-001".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: " operator-1 ".to_string(),
            source_type: SourceType::Erp,
            source_reference: Some(" msg-1 ".to_string()),
            reason_code: None,
            reason_text: None,
        }
    }

    /// happy path：字段规范化、指纹生成、FactBase 组合与状态机全链路。
    #[test]
    fn new_normalizes_fields_and_drives_state_machine() {
        let mut delivery = ElectronicDelivery::new(ElectronicDeliveryId::new("ed-1"), data()).unwrap();
        assert_eq!(delivery.fulfillment_no, "ED-2026-001");
        assert_eq!(delivery.fact.recorded_by, "operator-1");
        assert_eq!(delivery.fact.source_reference.as_deref(), Some("msg-1"));
        assert_eq!(delivery.fact.occurred_at.unix_secs(), 1_700_000_000);
        assert_eq!(delivery.status, ElectronicDeliveryState::Draft);

        delivery.confirm().unwrap();
        assert_eq!(delivery.status, ElectronicDeliveryState::Confirmed);
        delivery.reverse().unwrap();
        assert_eq!(delivery.status, ElectronicDeliveryState::Reversed);
    }

    /// 失败路径：必填空、超长、指纹格式非法、数量越界、时间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = ElectronicDeliveryData {
            fulfillment_no: "   ".to_string(),
            ..data()
        };
        assert!(ElectronicDelivery::new(ElectronicDeliveryId::new("ed-2"), blank_no).is_err());

        let overlong_snapshot = ElectronicDeliveryData {
            recipient_snapshot: "x".repeat(4097),
            ..data()
        };
        assert!(ElectronicDelivery::new(ElectronicDeliveryId::new("ed-3"), overlong_snapshot).is_err());

        let bad_fingerprint = ElectronicDeliveryData {
            recipient_snapshot_fingerprint: "zz".to_string(),
            ..data()
        };
        assert!(ElectronicDelivery::new(ElectronicDeliveryId::new("ed-4"), bad_fingerprint).is_err());

        let zero_quantity = ElectronicDeliveryData {
            quantity: Quantity::from_str("0").unwrap(),
            ..data()
        };
        assert!(ElectronicDelivery::new(ElectronicDeliveryId::new("ed-5"), zero_quantity).is_err());

        let reversed_time = ElectronicDeliveryData {
            recorded_at: Instant::from_unix_secs(1_699_999_999),
            ..data()
        };
        assert!(ElectronicDelivery::new(ElectronicDeliveryId::new("ed-6"), reversed_time).is_err());
    }

    /// 状态机：合法/非法/终态定向断言（含幂等迁移）。
    #[test]
    fn state_machine_directed_edges() {
        let mut delivery = ElectronicDelivery::new(ElectronicDeliveryId::new("ed-7"), data()).unwrap();
        assert!(delivery.reverse().is_err(), "草稿不能直接冲正");
        assert!(delivery
            .update(ElectronicDeliveryUpdate {
                result: Some(FulfillmentResult::Failure),
                evidence_attachment_id: None,
            })
            .is_ok());
        delivery.confirm().unwrap();
        // from == to 幂等迁移恒合法（state.rs 契约）；CONFIRMED 不可编辑由 update 把关。
        assert!(delivery.confirm().is_ok());
        assert!(
            delivery
                .update(ElectronicDeliveryUpdate {
                    result: None,
                    evidence_attachment_id: None,
                })
                .is_err(),
            "已确认不可编辑"
        );
        assert!(delivery.reverse().is_ok());
        assert!(
            delivery.reverse().is_ok(),
            "REVERSED 幂等迁移合法，且无法迁移到其他状态"
        );

        assert!(ensure_transition(ElectronicDeliveryState::Draft, ElectronicDeliveryState::Draft).is_ok());
        assert!(
            ensure_transition(ElectronicDeliveryState::Draft, ElectronicDeliveryState::Confirmed).is_ok()
        );
        assert!(ensure_transition(
            ElectronicDeliveryState::Confirmed,
            ElectronicDeliveryState::Reversed
        )
        .is_ok());
        assert!(
            ensure_transition(ElectronicDeliveryState::Draft, ElectronicDeliveryState::Reversed).is_err()
        );
        assert!(
            ensure_transition(ElectronicDeliveryState::Confirmed, ElectronicDeliveryState::Draft).is_err()
        );
        assert!(ensure_transition(
            ElectronicDeliveryState::Reversed,
            ElectronicDeliveryState::Confirmed
        )
        .is_err());
    }

    /// 敏感字段：指纹稳定且带密钥；Debug 不泄漏明文/密文/指纹。
    #[test]
    fn sensitive_fingerprint_and_redacted_debug() {
        let a = ElectronicDelivery::recipient_snapshot_fingerprint(PLAINTEXT_RECIPIENT, FINGERPRINT_KEY);
        assert_eq!(
            a,
            ElectronicDelivery::recipient_snapshot_fingerprint(PLAINTEXT_RECIPIENT, FINGERPRINT_KEY)
        );
        assert_ne!(
            a,
            ElectronicDelivery::recipient_snapshot_fingerprint(PLAINTEXT_RECIPIENT, b"other-key")
        );
        assert_eq!(a.len(), 64);

        let delivery = ElectronicDelivery::new(ElectronicDeliveryId::new("ed-8"), data()).unwrap();
        let debug = format!("{delivery:?}");
        assert!(!debug.contains(PLAINTEXT_RECIPIENT), "Debug 不得泄漏明文");
        assert!(!debug.contains("ciphertext-recipient"), "Debug 不得泄漏密文");
        assert!(!debug.contains(&a), "Debug 不得泄漏指纹");
        assert!(debug.contains("<redacted>"));
    }

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&FulfillmentResult::PartialSuccess).unwrap(),
            "\"PARTIAL_SUCCESS\""
        );
        assert_eq!(
            serde_json::to_string(&ElectronicDeliveryState::Confirmed).unwrap(),
            "\"CONFIRMED\""
        );
        assert_eq!(ElectronicDeliveryState::Confirmed.label(), "已确认");

        let delivery = ElectronicDelivery::new(ElectronicDeliveryId::new("ed-9"), data()).unwrap();
        let roundtrip: ElectronicDelivery =
            bson::from_document(bson::to_document(&delivery).unwrap()).unwrap();
        assert_eq!(roundtrip, delivery);
    }

    /// 电子交付无审批约束：不得出现绑定字段或审批状态机。
    #[test]
    fn electronic_delivery_has_no_approval_binding_or_state_machine() {
        let delivery = ElectronicDelivery::new(ElectronicDeliveryId::new("ed-1"), data()).unwrap();
        let value = serde_json::to_value(&delivery).unwrap();
        let object = value.as_object().expect("电子交付序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("pending_allocations"));
        assert_eq!(delivery.status, ElectronicDeliveryState::Draft);
        assert_eq!(ElectronicDeliveryState::Draft.as_str(), "DRAFT");
        assert_eq!(ElectronicDeliveryState::Confirmed.as_str(), "CONFIRMED");
        assert_eq!(ElectronicDeliveryState::Reversed.as_str(), "REVERSED");

        let production = include_str!("electronic_delivery.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("approval_subject_version"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("PENDING_REVIEW"));
    }
}
