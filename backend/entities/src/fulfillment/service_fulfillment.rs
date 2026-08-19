//! `service_fulfillment`：线下服务履约记录（数据模型 §6.7）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。
//!
//! 公共关键字段与 `electronic_delivery` 一致（含 `occurred_at` 等正式事实字段，
//! 组合 `FactBase`）；另保存服务地点、开始时间、结束时间和完成说明。
//! 服务地点是履约地址类敏感值，按 §4.5.5 建模为**加密值 + 带密钥 HMAC 查询
//! 指纹**两个字段，自定义 `Debug` 不泄漏。状态机按 §7.5：草稿 → 已确认 →
//! 已冲正。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::fact::FactBase;
use crate::common::source::SourceType;
use crate::common::state::{DocumentState, ensure_transition};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId, ServiceFulfillmentId,
};
use crate::money::Quantity;
use crate::validation::normalize_optional_text;
use crate::validation::normalize_required_text;

use super::electronic_delivery::FulfillmentResult;
use super::fingerprint::{FINGERPRINT_HEX_LEN, hmac_sha256_hex, validate_fingerprint};

/// 履约记录号最大长度。
const FULFILLMENT_NO_MAX_LEN: usize = 64;
/// 交付对象加密快照最大长度。
const RECIPIENT_SNAPSHOT_MAX_LEN: usize = 4096;
/// 服务地点加密值最大长度。
const SERVICE_LOCATION_ENCRYPTED_MAX_LEN: usize = 4096;
/// 完成说明最大长度。
const COMPLETION_NOTE_MAX_LEN: usize = 512;
/// 记录人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 线下服务履约状态（数据模型 §6.7：草稿、已确认、已冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServiceFulfillmentState {
    /// 草稿。
    Draft,
    /// 已确认（不可覆盖，纠错只能冲正）。
    Confirmed,
    /// 已冲正（不可逆终态）。
    Reversed,
}

impl ServiceFulfillmentState {
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

impl DocumentState for ServiceFulfillmentState {
    /// 固定邻接矩阵（§7.5 定向链，`REVERSED` 为不可逆终态）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Confirmed],
            Self::Confirmed => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 线下服务履约记录创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceFulfillmentData {
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
    /// 服务数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 服务地点加密值（P3 加密生成；本层只定义结构）。
    pub service_location_encrypted: String,
    /// 服务地点带密钥 HMAC 查询指纹（64 位十六进制）。
    pub service_location_fingerprint: String,
    /// 服务开始时间。
    pub service_started_at: Option<Instant>,
    /// 服务结束时间（不得早于开始时间）。
    pub service_ended_at: Option<Instant>,
    /// 完成说明。
    pub completion_note: Option<String>,
    /// 聚合内稳定序号（正式事实，§4.3）。
    pub fact_no: String,
    /// 实际服务时间。
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

/// 线下服务履约记录更新数据（仅草稿可更新）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceFulfillmentUpdate {
    /// 履约结果；`None` 表示不修改。
    pub result: Option<FulfillmentResult>,
    /// 完成说明；`None` 表示不修改。
    pub completion_note: Option<String>,
}

/// 线下服务履约记录实体（数据模型 §6.7）。
///
/// 组合 `FactBase` 表达正式事实语义（§4.3）；已确认记录不可覆盖，失败后重做
/// 形成新记录；冲正由反向事实表达（§6.7）。`Debug` 输出不泄漏交付对象快照、
/// 服务地点及其指纹。
#[derive(Serialize, Deserialize, Clone, Entity)]
pub struct ServiceFulfillment {
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
    /// 服务数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 服务地点加密值。
    pub service_location_encrypted: String,
    /// 服务地点带密钥 HMAC 查询指纹。
    pub service_location_fingerprint: String,
    /// 服务开始时间。
    pub service_started_at: Option<Instant>,
    /// 服务结束时间。
    pub service_ended_at: Option<Instant>,
    /// 完成说明。
    pub completion_note: Option<String>,
    /// 当前状态。
    pub status: ServiceFulfillmentState,
}

impl fmt::Debug for ServiceFulfillment {
    /// 脱敏调试输出：交付对象快照、服务地点及指纹一律以 `<redacted>` 呈现。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceFulfillment")
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
            .field("service_location_encrypted", &"<redacted>")
            .field("service_location_fingerprint", &"<redacted>")
            .field("service_started_at", &self.service_started_at)
            .field("service_ended_at", &self.service_ended_at)
            .field("completion_note", &self.completion_note)
            .field("status", &self.status)
            .finish()
    }
}

impl PartialEq for ServiceFulfillment {
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
            && self.service_location_encrypted == other.service_location_encrypted
            && self.service_location_fingerprint == other.service_location_fingerprint
            && self.service_started_at == other.service_started_at
            && self.service_ended_at == other.service_ended_at
            && self.completion_note == other.completion_note
            && self.status == other.status
    }
}

impl Eq for ServiceFulfillment {}

impl ServiceFulfillment {
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

    /// 生成服务地点查询指纹。
    ///
    /// 对规范化后的服务地点原文字符串计算带密钥 HMAC-SHA256（§4.5.5，
    /// 禁止裸摘要）；密钥不持久化，精确查询时用同一密钥比对指纹。
    ///
    /// # 参数
    /// * `plain` - 服务地点原文字符串
    /// * `key` - 查询指纹密钥字节
    ///
    /// # 返回
    /// 返回 64 位小写十六进制指纹。
    pub fn service_location_fingerprint(plain: &str, key: &[u8]) -> String {
        hmac_sha256_hex(key, plain.as_bytes())
    }

    /// 创建线下服务履约记录（初始状态为草稿）。
    ///
    /// 完成履约记录号、记录人、完成说明与快照指纹格式校验；校验
    /// `recorded_at` 不早于 `occurred_at`、服务结束时间不早于开始时间。
    /// 已确认记录必须引用同一销售明细和采购单的有效采购销售分配——跨聚合
    /// 校验由 P3 完成（§6.7）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ServiceFulfillmentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的服务履约记录实体。
    ///
    /// # 错误
    /// 必填字段为空/超长、指纹格式非法、服务数量非正、记录时间早于发生时间
    /// 或服务结束时间早于开始时间时返回错误。
    pub fn new(id: ServiceFulfillmentId, data: ServiceFulfillmentData) -> Result<Self> {
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
        let service_location_encrypted = normalize_required_text(
            data.service_location_encrypted,
            "服务地点加密值不能为空",
            SERVICE_LOCATION_ENCRYPTED_MAX_LEN,
            "服务地点加密值过长",
        )?;
        let service_location_fingerprint = normalize_required_text(
            data.service_location_fingerprint,
            "服务地点查询指纹不能为空",
            FINGERPRINT_HEX_LEN,
            "服务地点查询指纹过长",
        )?;
        validate_fingerprint(&service_location_fingerprint)?;
        let recorded_by =
            normalize_required_text(data.recorded_by, "记录人不能为空", ACTOR_MAX_LEN, "记录人过长")?;
        let completion_note =
            normalize_optional_text(data.completion_note, "完成说明", COMPLETION_NOTE_MAX_LEN)?;
        if data.recorded_at < data.occurred_at {
            return Err(Error::from("记录时间不得早于实际服务时间"));
        }
        if data.quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("服务数量必须为正数"));
        }
        if let (Some(started_at), Some(ended_at)) = (data.service_started_at, data.service_ended_at) {
            if ended_at < started_at {
                return Err(Error::from("服务结束时间不得早于开始时间"));
            }
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            fact: FactBase::new(
                data.fact_no,
                data.occurred_at,
                data.recorded_at,
                recorded_by,
                data.source_type,
                data.source_reference,
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
            service_location_encrypted,
            service_location_fingerprint,
            service_started_at: data.service_started_at,
            service_ended_at: data.service_ended_at,
            completion_note,
            status: ServiceFulfillmentState::Draft,
        })
    }

    /// 更新线下服务履约记录（仅草稿）。
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
    /// 状态不可编辑，或完成说明超长时返回错误。
    pub fn update(&mut self, update: ServiceFulfillmentUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(result) = update.result {
            self.result = result;
        }
        if let Some(completion_note) = update.completion_note {
            self.completion_note =
                normalize_optional_text(Some(completion_note), "完成说明", COMPLETION_NOTE_MAX_LEN)?;
        }
        Ok(())
    }

    /// 确认服务履约（草稿 → 已确认）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非草稿）时返回错误。
    pub fn confirm(&mut self) -> Result<()> {
        ensure_transition(self.status, ServiceFulfillmentState::Confirmed)?;
        self.status = ServiceFulfillmentState::Confirmed;
        Ok(())
    }

    /// 冲正服务履约（已确认 → 已冲正，终态）。
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
        ensure_transition(self.status, ServiceFulfillmentState::Reversed)?;
        self.status = ServiceFulfillmentState::Reversed;
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
            return Err(Error::from("已确认或已冲正的服务履约记录不可覆盖"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ServiceFulfillmentId;
    use std::str::FromStr;

    const PLAINTEXT_RECIPIENT: &str = "收货人 王五 13912345678";
    const PLAINTEXT_LOCATION: &str = "上海市徐汇区漕河泾开发区xx大厦 3F 会议室A";
    const FINGERPRINT_KEY: &[u8] = b"test-fingerprint-key";

    fn data() -> ServiceFulfillmentData {
        ServiceFulfillmentData {
            fulfillment_no: " SF-2026-001 ".to_string(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            recipient_snapshot: "ciphertext-recipient...".to_string(),
            recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                PLAINTEXT_RECIPIENT,
                FINGERPRINT_KEY,
            ),
            quantity: Quantity::from_str("1").unwrap(),
            result: FulfillmentResult::Success,
            evidence_attachment_id: Some(FileAssetId::new("file-1")),
            service_location_encrypted: "ciphertext-location...".to_string(),
            service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                PLAINTEXT_LOCATION,
                FINGERPRINT_KEY,
            ),
            service_started_at: Some(Instant::from_unix_secs(1_700_000_000)),
            service_ended_at: Some(Instant::from_unix_secs(1_700_003_600)),
            completion_note: Some(" 上门安装调试完成 ".to_string()),
            fact_no: "F-002".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: "operator-1".to_string(),
            source_type: SourceType::ManualImport,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        }
    }

    /// happy path：字段规范化、双指纹生成、FactBase 组合与状态机全链路。
    #[test]
    fn new_normalizes_fields_and_drives_state_machine() {
        let mut fulfillment = ServiceFulfillment::new(ServiceFulfillmentId::new("sf-1"), data()).unwrap();
        assert_eq!(fulfillment.fulfillment_no, "SF-2026-001");
        assert_eq!(fulfillment.completion_note.as_deref(), Some("上门安装调试完成"));
        assert_eq!(fulfillment.fact.source_type, SourceType::ManualImport);
        assert_eq!(fulfillment.status, ServiceFulfillmentState::Draft);

        fulfillment.confirm().unwrap();
        assert_eq!(fulfillment.status, ServiceFulfillmentState::Confirmed);
        fulfillment.reverse().unwrap();
        assert_eq!(fulfillment.status, ServiceFulfillmentState::Reversed);
    }

    /// 失败路径：必填空、指纹格式非法、服务时间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = ServiceFulfillmentData {
            fulfillment_no: "   ".to_string(),
            ..data()
        };
        assert!(ServiceFulfillment::new(ServiceFulfillmentId::new("sf-2"), blank_no).is_err());

        let bad_location_fingerprint = ServiceFulfillmentData {
            service_location_fingerprint: "bad".to_string(),
            ..data()
        };
        assert!(
            ServiceFulfillment::new(ServiceFulfillmentId::new("sf-3"), bad_location_fingerprint).is_err()
        );

        let reversed_window = ServiceFulfillmentData {
            service_started_at: Some(Instant::from_unix_secs(1_700_003_600)),
            service_ended_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..data()
        };
        assert!(ServiceFulfillment::new(ServiceFulfillmentId::new("sf-4"), reversed_window).is_err());

        let zero_quantity = ServiceFulfillmentData {
            quantity: Quantity::from_str("0").unwrap(),
            ..data()
        };
        assert!(ServiceFulfillment::new(ServiceFulfillmentId::new("sf-5"), zero_quantity).is_err());

        let overlong_note = ServiceFulfillmentData {
            completion_note: Some("x".repeat(513)),
            ..data()
        };
        assert!(ServiceFulfillment::new(ServiceFulfillmentId::new("sf-6"), overlong_note).is_err());
    }

    /// 状态机：合法/非法/终态定向断言（含幂等迁移）。
    #[test]
    fn state_machine_directed_edges() {
        let mut fulfillment = ServiceFulfillment::new(ServiceFulfillmentId::new("sf-7"), data()).unwrap();
        assert!(fulfillment.reverse().is_err(), "草稿不能直接冲正");
        assert!(
            fulfillment
                .update(ServiceFulfillmentUpdate {
                    result: None,
                    completion_note: Some("   ".to_string()),
                })
                .is_ok(),
            "空完成说明视为清除"
        );
        fulfillment.confirm().unwrap();
        // from == to 幂等迁移恒合法（state.rs 契约）；CONFIRMED 不可编辑由 update 把关。
        assert!(fulfillment.confirm().is_ok());
        assert!(
            fulfillment
                .update(ServiceFulfillmentUpdate {
                    result: None,
                    completion_note: None,
                })
                .is_err(),
            "已确认不可编辑"
        );
        assert!(fulfillment.reverse().is_ok());
        assert!(
            fulfillment.reverse().is_ok(),
            "REVERSED 幂等迁移合法，且无法迁移到其他状态"
        );

        assert!(
            ensure_transition(ServiceFulfillmentState::Draft, ServiceFulfillmentState::Confirmed).is_ok()
        );
        assert!(
            ensure_transition(
                ServiceFulfillmentState::Confirmed,
                ServiceFulfillmentState::Reversed
            )
            .is_ok()
        );
        assert!(
            ensure_transition(ServiceFulfillmentState::Draft, ServiceFulfillmentState::Reversed).is_err()
        );
        assert!(
            ensure_transition(ServiceFulfillmentState::Reversed, ServiceFulfillmentState::Draft).is_err()
        );
    }

    /// 敏感字段：双指纹稳定且带密钥；Debug 不泄漏明文/密文/指纹。
    #[test]
    fn sensitive_fingerprints_and_redacted_debug() {
        let recipient_fp =
            ServiceFulfillment::recipient_snapshot_fingerprint(PLAINTEXT_RECIPIENT, FINGERPRINT_KEY);
        let location_fp =
            ServiceFulfillment::service_location_fingerprint(PLAINTEXT_LOCATION, FINGERPRINT_KEY);
        assert_eq!(
            recipient_fp,
            ServiceFulfillment::recipient_snapshot_fingerprint(PLAINTEXT_RECIPIENT, FINGERPRINT_KEY)
        );
        assert_ne!(
            location_fp,
            ServiceFulfillment::service_location_fingerprint(PLAINTEXT_LOCATION, b"other-key")
        );
        assert_ne!(recipient_fp, location_fp);

        let fulfillment = ServiceFulfillment::new(ServiceFulfillmentId::new("sf-8"), data()).unwrap();
        let debug = format!("{fulfillment:?}");
        assert!(!debug.contains(PLAINTEXT_RECIPIENT));
        assert!(!debug.contains(PLAINTEXT_LOCATION));
        assert!(!debug.contains("ciphertext-recipient"));
        assert!(!debug.contains("ciphertext-location"));
        assert!(!debug.contains(&recipient_fp));
        assert!(!debug.contains(&location_fp));
        assert!(debug.contains("<redacted>"));
    }

    /// 序列化：状态枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&ServiceFulfillmentState::Confirmed).unwrap(),
            "\"CONFIRMED\""
        );
        assert_eq!(ServiceFulfillmentState::Reversed.label(), "已冲正");

        let fulfillment = ServiceFulfillment::new(ServiceFulfillmentId::new("sf-9"), data()).unwrap();
        let roundtrip: ServiceFulfillment =
            bson::from_document(bson::to_document(&fulfillment).unwrap()).unwrap();
        assert_eq!(roundtrip, fulfillment);
    }
}
