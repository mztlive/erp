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
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId, ServiceFulfillmentId,
};
use crate::money::Quantity;
use crate::validation::normalize_optional_text;
use crate::validation::normalize_required_text;

use super::electronic_delivery::FulfillmentResult;
use super::fingerprint::{hmac_sha256_hex, validate_fingerprint, FINGERPRINT_HEX_LEN};

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

    /// 判断当前状态能否执行首次确认。
    ///
    /// # 返回
    /// 仅草稿状态返回 `true`。
    pub fn is_confirmable(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// 判断当前状态能否作为客户验收履约事实。
    ///
    /// # 返回
    /// 已确认状态返回 `true`。
    pub fn is_acceptance_eligible(self) -> bool {
        matches!(self, Self::Confirmed)
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

/// 确认线下服务履约前必须写全的现场事实。
///
/// 采购审核只生成占位草稿；确认时必须登记地点、时间窗、完成说明、数量和
/// 图片凭证。凭证以 `file_asset` 主键引用，本结构不持有文件字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceFulfillmentConfirmation {
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 完成说明。
    pub completion_note: String,
    /// 现场图片凭证。
    pub evidence_attachment_id: FileAssetId,
    /// 服务地点加密/不透明值。
    pub service_location_encrypted: String,
    /// 服务地点查询指纹。
    pub service_location_fingerprint: String,
    /// 服务开始时间。
    pub service_started_at: Instant,
    /// 服务结束时间。
    pub service_ended_at: Instant,
    /// 本次完成数量。
    pub quantity: Quantity,
}

impl ServiceFulfillmentConfirmation {
    /// 规范化并校验确认现场事实。
    ///
    /// # 参数
    /// * `result` - 履约结果
    /// * `completion_note` - 完成说明
    /// * `evidence_attachment_id` - 现场图片凭证
    /// * `service_location_encrypted` - 服务地点加密/不透明值
    /// * `service_location_fingerprint` - 服务地点查询指纹
    /// * `service_started_at` - 服务开始时间
    /// * `service_ended_at` - 服务结束时间
    /// * `quantity` - 本次完成数量
    ///
    /// # 返回
    /// 返回可写入草稿的确认事实。
    ///
    /// # 错误
    /// 结果为部分成功、说明/地点/指纹为空或超长、指纹格式非法、数量非正、
    /// 结束早于开始或凭证主键为空时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        result: FulfillmentResult,
        completion_note: String,
        evidence_attachment_id: FileAssetId,
        service_location_encrypted: String,
        service_location_fingerprint: String,
        service_started_at: Instant,
        service_ended_at: Instant,
        quantity: Quantity,
    ) -> Result<Self> {
        let result = ensure_binary_service_result(result)?;
        let completion_note = normalize_required_text(
            completion_note,
            "完成说明不能为空",
            COMPLETION_NOTE_MAX_LEN,
            "完成说明过长",
        )?;
        let evidence_attachment_id = require_evidence_attachment_id(evidence_attachment_id)?;
        let service_location_encrypted = normalize_required_text(
            service_location_encrypted,
            "服务地点加密值不能为空",
            SERVICE_LOCATION_ENCRYPTED_MAX_LEN,
            "服务地点加密值过长",
        )?;
        let service_location_fingerprint = normalize_required_text(
            service_location_fingerprint,
            "服务地点查询指纹不能为空",
            FINGERPRINT_HEX_LEN,
            "服务地点查询指纹过长",
        )?;
        validate_fingerprint(&service_location_fingerprint)?;
        ensure_positive_quantity(&quantity)?;
        ensure_service_window(service_started_at, service_ended_at)?;
        Ok(Self {
            result,
            completion_note,
            evidence_attachment_id,
            service_location_encrypted,
            service_location_fingerprint,
            service_started_at,
            service_ended_at,
            quantity,
        })
    }
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
    /// Service 加载采购单、当前版本和采购销售分配后，由履约资格值对象校验
    /// 关联；本实体负责确认与验收事实状态。
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
        ensure_positive_quantity(&data.quantity)?;
        if let (Some(started_at), Some(ended_at)) = (data.service_started_at, data.service_ended_at) {
            ensure_service_window(started_at, ended_at)?;
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            fact: FactBase::new(
                data.fact_no,
                data.occurred_at,
                data.recorded_at,
                recorded_by,
                crate::common::fact::FactSource {
                    source_type: data.source_type,
                    source_reference: data.source_reference,
                    reason_code: data.reason_code,
                    reason_text: data.reason_text,
                },
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

    /// 返回单据注册与无审批绑定重验使用的组织上下文。
    ///
    /// # 返回
    /// 返回所属销售稳定明细主键。
    ///
    /// # 错误
    /// 销售稳定明细主键为空时返回错误。
    pub fn registration_context_id(&self) -> Result<&str> {
        let id = self.sales_order_line_id.as_ref();
        if id.trim().is_empty() {
            return Err(Error::from("服务履约缺少销售明细，无法构造绑定上下文"));
        }
        Ok(id)
    }

    /// 校验记录可执行首次确认。
    ///
    /// # 返回
    /// 草稿状态返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿状态返回错误。
    pub fn ensure_confirmable(&self) -> Result<()> {
        if !self.status.is_confirmable() {
            return Err(Error::from("只有草稿状态的服务履约记录可以确认"));
        }
        Ok(())
    }

    /// 校验记录仍为调用方看到的草稿版本。
    ///
    /// # 参数
    /// * `expected_version` - 调用方提交的乐观锁版本
    ///
    /// # 返回
    /// 草稿且版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是草稿或版本不一致时返回错误。
    pub fn ensure_draft_version(&self, expected_version: u64) -> Result<()> {
        self.ensure_confirmable()?;
        if self.base.version != expected_version {
            return Err(Error::from("服务履约记录版本已变化，请刷新后重试"));
        }
        Ok(())
    }

    /// 校验确认前已登记图片凭证。
    ///
    /// # 返回
    /// 凭证主键非空时返回 `Ok(())`。
    ///
    /// # 错误
    /// 未上传图片凭证时返回错误。
    pub fn ensure_evidence_present(&self) -> Result<()> {
        let Some(evidence_attachment_id) = self.evidence_attachment_id.clone() else {
            return Err(Error::from("线下服务履约必须上传图片凭证"));
        };
        require_evidence_attachment_id(evidence_attachment_id)?;
        Ok(())
    }

    /// 校验服务履约可作为指定验收行的履约事实并返回成功数量。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 验收行所属销售稳定明细
    ///
    /// # 返回
    /// 履约成功、已确认且销售明细一致时返回服务数量。
    ///
    /// # 错误
    /// 履约失败、状态无效或销售明细关联不一致时返回错误。
    pub fn acceptance_quantity(&self, sales_order_line_id: &SalesOrderLineId) -> Result<Quantity> {
        if !self.is_acceptance_eligible() {
            return Err(Error::from("服务履约事实未成功确认"));
        }
        if self.sales_order_line_id != *sales_order_line_id {
            return Err(Error::from("履约事实不属于本验收明细"));
        }
        Ok(self.quantity)
    }

    /// 判断服务履约事实是否可进入客户验收。
    ///
    /// # 返回
    /// 仅已确认且履约结果成功时返回 `true`；失败记录只保留尝试事实。
    pub fn is_acceptance_eligible(&self) -> bool {
        self.status.is_acceptance_eligible() && self.result == FulfillmentResult::Success
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

    /// 把确认现场事实写入草稿（仅草稿）。
    ///
    /// 覆盖占位地点、时间窗、完成说明和图片凭证；确认数量必须等于冻结的
    /// 采购销售分配数量，不在确认时改写计划数量。
    ///
    /// # 参数
    /// * `confirmation` - 已规范化的确认现场事实
    ///
    /// # 返回
    /// 写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不可编辑，或确认数量与冻结分配数量不一致时返回错误。
    pub fn apply_confirmation(&mut self, confirmation: ServiceFulfillmentConfirmation) -> Result<()> {
        self.ensure_editable()?;
        if confirmation.quantity != self.quantity {
            return Err(Error::from("服务完成数量必须与采购销售分配数量一致"));
        }
        self.result = confirmation.result;
        self.completion_note = Some(confirmation.completion_note);
        self.evidence_attachment_id = Some(confirmation.evidence_attachment_id);
        self.service_location_encrypted = confirmation.service_location_encrypted;
        self.service_location_fingerprint = confirmation.service_location_fingerprint;
        self.service_started_at = Some(confirmation.service_started_at);
        self.service_ended_at = Some(confirmation.service_ended_at);
        Ok(())
    }

    /// 确认服务履约（草稿 → 已确认）。
    ///
    /// 确认前必须已写入图片凭证；现场地点和时间由 [`Self::apply_confirmation`]
    /// 在同一确认命令内写入。已确认记录再次确认保持幂等。
    ///
    /// # 返回
    /// 迁移成功或已确认幂等时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移，或草稿尚未上传图片凭证时返回错误。
    pub fn confirm(&mut self) -> Result<()> {
        if self.status == ServiceFulfillmentState::Confirmed {
            return Ok(());
        }
        self.ensure_confirmable()?;
        self.ensure_evidence_present()?;
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

/// 校验服务数量为正。
///
/// # 参数
/// * `quantity` - 待校验数量
///
/// # 返回
/// 数量为正时返回 `Ok(())`。
///
/// # 错误
/// 数量小于或等于零时返回错误。
fn ensure_positive_quantity(quantity: &Quantity) -> Result<()> {
    if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("服务数量必须为正数"));
    }
    Ok(())
}

/// 校验服务结束时间不早于开始时间。
///
/// # 参数
/// * `started_at` - 服务开始时间
/// * `ended_at` - 服务结束时间
///
/// # 返回
/// 时间窗合法时返回 `Ok(())`。
///
/// # 错误
/// 结束早于开始时返回错误。
fn ensure_service_window(started_at: Instant, ended_at: Instant) -> Result<()> {
    if ended_at < started_at {
        return Err(Error::from("服务结束时间不得早于开始时间"));
    }
    Ok(())
}

/// 校验线下服务履约结果只能是成功或失败。
///
/// # 参数
/// * `result` - 待确认的履约结果
///
/// # 返回
/// 成功或失败时原样返回。
///
/// # 错误
/// 结果为部分成功时返回错误。
fn ensure_binary_service_result(result: FulfillmentResult) -> Result<FulfillmentResult> {
    if matches!(result, FulfillmentResult::PartialSuccess) {
        return Err(Error::from("线下服务履约结果只能是成功或失败"));
    }
    Ok(result)
}

/// 校验图片凭证主键非空。
///
/// # 参数
/// * `evidence_attachment_id` - 文件资产主键
///
/// # 返回
/// 主键非空时原样返回。
///
/// # 错误
/// 主键空白时返回错误。
fn require_evidence_attachment_id(evidence_attachment_id: FileAssetId) -> Result<FileAssetId> {
    if evidence_attachment_id.as_ref().trim().is_empty() {
        return Err(Error::from("线下服务履约必须上传图片凭证"));
    }
    Ok(evidence_attachment_id)
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
        assert!(ensure_transition(
            ServiceFulfillmentState::Confirmed,
            ServiceFulfillmentState::Reversed
        )
        .is_ok());
        assert!(
            ensure_transition(ServiceFulfillmentState::Draft, ServiceFulfillmentState::Reversed).is_err()
        );
        assert!(
            ensure_transition(ServiceFulfillmentState::Reversed, ServiceFulfillmentState::Draft).is_err()
        );
    }

    /// 草稿确认必须先写入图片凭证；已确认幂等，冲正后不可再确认。
    #[test]
    fn confirm_requires_image_evidence_and_stays_idempotent() {
        let mut missing_evidence = ServiceFulfillment::new(
            ServiceFulfillmentId::new("sf-evidence"),
            ServiceFulfillmentData {
                evidence_attachment_id: None,
                ..data()
            },
        )
        .unwrap();
        assert!(missing_evidence.ensure_evidence_present().is_err());
        assert!(missing_evidence.confirm().is_err());

        let confirmation = ServiceFulfillmentConfirmation::new(
            FulfillmentResult::Success,
            "上门安装调试完成".to_string(),
            FileAssetId::new("file-confirm"),
            "ciphertext-location-confirmed".to_string(),
            ServiceFulfillment::service_location_fingerprint(PLAINTEXT_LOCATION, FINGERPRINT_KEY),
            Instant::from_unix_secs(1_700_000_000),
            Instant::from_unix_secs(1_700_003_600),
            Quantity::from_str("1").unwrap(),
        )
        .unwrap();
        missing_evidence.apply_confirmation(confirmation).unwrap();
        assert_eq!(
            missing_evidence.evidence_attachment_id,
            Some(FileAssetId::new("file-confirm"))
        );
        missing_evidence.confirm().unwrap();
        assert_eq!(missing_evidence.status, ServiceFulfillmentState::Confirmed);
        assert!(missing_evidence.confirm().is_ok());
        assert!(missing_evidence
            .ensure_draft_version(missing_evidence.base.version)
            .is_err());
    }

    /// 确认不得放大或缩小采购销售分配冻结的服务数量。
    #[test]
    fn confirmation_quantity_must_match_frozen_allocation() {
        let fingerprint =
            ServiceFulfillment::service_location_fingerprint(PLAINTEXT_LOCATION, FINGERPRINT_KEY);
        for quantity in ["0.5", "100"] {
            let mut fulfillment =
                ServiceFulfillment::new(ServiceFulfillmentId::new(format!("sf-{quantity}")), data()).unwrap();
            let confirmation = ServiceFulfillmentConfirmation::new(
                FulfillmentResult::Success,
                "上门安装调试完成".to_string(),
                FileAssetId::new("file-confirm"),
                "ciphertext-location-confirmed".to_string(),
                fingerprint.clone(),
                Instant::from_unix_secs(1_700_000_000),
                Instant::from_unix_secs(1_700_003_600),
                Quantity::from_str(quantity).unwrap(),
            )
            .unwrap();

            assert!(fulfillment.apply_confirmation(confirmation).is_err());
            assert_eq!(fulfillment.quantity, Quantity::from_str("1").unwrap());
            assert_eq!(fulfillment.status, ServiceFulfillmentState::Draft);
        }
    }

    /// 确认现场事实拒绝空凭证、倒挂时间窗和非正数量。
    #[test]
    fn confirmation_facts_reject_invalid_inputs() {
        let fingerprint =
            ServiceFulfillment::service_location_fingerprint(PLAINTEXT_LOCATION, FINGERPRINT_KEY);
        assert!(ServiceFulfillmentConfirmation::new(
            FulfillmentResult::Success,
            "上门安装调试完成".to_string(),
            FileAssetId::new("   "),
            "ciphertext-location".to_string(),
            fingerprint.clone(),
            Instant::from_unix_secs(1_700_000_000),
            Instant::from_unix_secs(1_700_003_600),
            Quantity::from_str("1").unwrap(),
        )
        .is_err());
        assert!(ServiceFulfillmentConfirmation::new(
            FulfillmentResult::Success,
            "上门安装调试完成".to_string(),
            FileAssetId::new("file-1"),
            "ciphertext-location".to_string(),
            fingerprint.clone(),
            Instant::from_unix_secs(1_700_003_600),
            Instant::from_unix_secs(1_700_000_000),
            Quantity::from_str("1").unwrap(),
        )
        .is_err());
        assert!(ServiceFulfillmentConfirmation::new(
            FulfillmentResult::PartialSuccess,
            "上门安装调试完成".to_string(),
            FileAssetId::new("file-1"),
            "ciphertext-location".to_string(),
            fingerprint,
            Instant::from_unix_secs(1_700_000_000),
            Instant::from_unix_secs(1_700_003_600),
            Quantity::from_str("1").unwrap(),
        )
        .is_err());
    }

    /// 确认与验收资格由实体状态及销售明细关联共同决定。
    #[test]
    fn confirmation_and_acceptance_rules_are_entity_owned() {
        let mut fulfillment = ServiceFulfillment::new(ServiceFulfillmentId::new("sf-rule"), data()).unwrap();
        assert!(fulfillment.ensure_confirmable().is_ok());
        assert!(fulfillment
            .acceptance_quantity(&SalesOrderLineId::new("so-line-1"))
            .is_err());
        fulfillment.confirm().unwrap();
        assert!(fulfillment.ensure_confirmable().is_err());
        assert!(fulfillment.is_acceptance_eligible());
        assert_eq!(
            fulfillment
                .acceptance_quantity(&SalesOrderLineId::new("so-line-1"))
                .unwrap(),
            Quantity::from_str("1").unwrap()
        );
        assert!(fulfillment
            .acceptance_quantity(&SalesOrderLineId::new("other-line"))
            .is_err());
        assert_eq!(fulfillment.registration_context_id().unwrap(), "so-line-1");
        let mut missing_context = fulfillment.clone();
        missing_context.sales_order_line_id = SalesOrderLineId::new("   ");
        assert!(missing_context.registration_context_id().is_err());

        let mut failed = ServiceFulfillment::new(
            ServiceFulfillmentId::new("sf-failed"),
            ServiceFulfillmentData {
                result: FulfillmentResult::Failure,
                ..data()
            },
        )
        .unwrap();
        failed.confirm().unwrap();
        assert!(!failed.is_acceptance_eligible());
        assert!(failed
            .acceptance_quantity(&SalesOrderLineId::new("so-line-1"))
            .is_err());
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
            bson::deserialize_from_document(bson::serialize_to_document(&fulfillment).unwrap()).unwrap();
        assert_eq!(roundtrip, fulfillment);
    }
}
