//! `delivery` / `delivery_line`：履约发货单及行（数据模型 §6.7）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。
//!
//! - 状态机按 §6.7/§7.5：草稿 → 已发货 → 已签收 → 已冲正（`SHIPPED`/`SIGNED`
//!   均可冲正，`REVERSED` 为不可逆终态）；已发货事实不因销售或采购变更被删除；
//! - 履约地址等敏感值按 §4.5.5/P1 §2.1 建模为**加密值 + 带密钥 HMAC 查询指纹**
//!   两个字段（字段命名沿用 §6.18/§6.19 的 `address_snapshot_encrypted` 惯例，
//!   字典未为 delivery 单独列地址字段），自定义 `Debug` 不输出明文/密文/指纹；
//! - 仓发必须消耗本销售明细的有效预占并形成出库流水、供应商直发不得写自有库存
//!   流水——跨聚合动作由 P3 完成（§8.2 第 2 条）；累计有效发货不得超过变更后
//!   有效销售数量为跨聚合校验（P3）。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    DeliveryId, DeliveryLineId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderId,
    SalesOrderLineId, StockReservationId, WarehouseId,
};
use crate::money::Quantity;
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::fingerprint::{hmac_sha256_hex, validate_fingerprint};

/// 发货单号最大长度。
const DELIVERY_NO_MAX_LEN: usize = 64;
/// 物流承运方最大长度。
const CARRIER_MAX_LEN: usize = 64;
/// 物流单号最大长度。
const TRACKING_NO_MAX_LEN: usize = 128;
/// 履约地址加密值最大长度。
const ADDRESS_ENCRYPTED_MAX_LEN: usize = 4096;

/// 发货单状态（数据模型 §6.7：草稿、已发货、已签收、已冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryState {
    /// 草稿。
    Draft,
    /// 已发货。
    Shipped,
    /// 已签收。
    Signed,
    /// 已冲正（不可逆终态）。
    Reversed,
}

impl DeliveryState {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Shipped => "已发货",
            Self::Signed => "已签收",
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
            Self::Shipped => "SHIPPED",
            Self::Signed => "SIGNED",
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

    /// 返回可作为客户验收依据的发货状态集合。
    ///
    /// # 返回
    /// 固定返回已发货与已签收状态。
    pub fn acceptance_eligible_states() -> &'static [Self] {
        &[Self::Shipped, Self::Signed]
    }

    /// 判断当前状态能否作为客户验收履约事实。
    ///
    /// # 返回
    /// 已发货或已签收时返回 `true`。
    pub fn is_acceptance_eligible(self) -> bool {
        Self::acceptance_eligible_states().contains(&self)
    }
}

impl DocumentState for DeliveryState {
    /// 固定邻接矩阵（§7.5 定向链，`REVERSED` 为不可逆终态）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Shipped],
            Self::Shipped => &[Self::Signed, Self::Reversed],
            Self::Signed => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 发货类型（数据模型 §6.7：`WAREHOUSE_SHIP` 或 `SUPPLIER_DIRECT`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryType {
    /// 仓发：从公司仓库发货，消耗预占并形成出库流水。
    WarehouseShip,
    /// 供应商直发：由供应商直接发客户，不写自有库存流水。
    SupplierDirect,
}

impl DeliveryType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::WarehouseShip => "仓发",
            Self::SupplierDirect => "供应商直发",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WarehouseShip => "WAREHOUSE_SHIP",
            Self::SupplierDirect => "SUPPLIER_DIRECT",
        }
    }
}

/// 发货单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryData {
    /// 履约发货单号（全局唯一）。
    pub delivery_no: String,
    /// 发货类型。
    pub delivery_type: DeliveryType,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 供应商直发时的采购来源；仓发为空。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 入库仓；仓发必填，直发为空。
    pub warehouse_id: Option<WarehouseId>,
    /// 物流承运方。
    pub carrier: Option<String>,
    /// 物流单号。
    pub tracking_no: Option<String>,
    /// 履约地址加密值（P3 由交付目标地址加密生成；本层只定义结构）。
    pub address_snapshot_encrypted: Option<String>,
    /// 履约地址带密钥 HMAC 查询指纹（64 位十六进制）。
    pub address_snapshot_fingerprint: Option<String>,
}

/// 发货单更新数据（仅草稿可更新）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryUpdate {
    /// 物流承运方；`None` 表示不修改。
    pub carrier: Option<String>,
    /// 物流单号；`None` 表示不修改。
    pub tracking_no: Option<String>,
}

/// 履约发货单实体（数据模型 §6.7 表头）。
///
/// 仓发/直发的 `purchase_order_id` 与 `warehouse_id` 归属按字典校验；
/// 履约地址按 §4.5.5 保存加密值与查询指纹，`Debug` 输出不泄漏敏感值。
/// 已发货/已签收/已冲正是正式事实，不设业务软删除（§4.5.1）。
#[derive(Serialize, Deserialize, Clone, Entity)]
pub struct Delivery {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 履约发货单号。
    pub delivery_no: String,
    /// 发货类型。
    pub delivery_type: DeliveryType,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 供应商直发时的采购来源。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 仓发时的入库仓。
    pub warehouse_id: Option<WarehouseId>,
    /// 当前状态。
    pub status: DeliveryState,
    /// 物流承运方。
    pub carrier: Option<String>,
    /// 物流单号。
    pub tracking_no: Option<String>,
    /// 发货时间。
    pub shipped_at: Option<Instant>,
    /// 履约地址加密值。
    pub address_snapshot_encrypted: Option<String>,
    /// 履约地址带密钥 HMAC 查询指纹。
    pub address_snapshot_fingerprint: Option<String>,
}

impl fmt::Debug for Delivery {
    /// 脱敏调试输出：履约地址的密文与指纹一律以 `<redacted>` 呈现。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Delivery")
            .field("base", &self.base)
            .field("delivery_no", &self.delivery_no)
            .field("delivery_type", &self.delivery_type)
            .field("sales_order_id", &self.sales_order_id)
            .field("purchase_order_id", &self.purchase_order_id)
            .field("warehouse_id", &self.warehouse_id)
            .field("status", &self.status)
            .field("carrier", &self.carrier)
            .field("tracking_no", &self.tracking_no)
            .field("shipped_at", &self.shipped_at)
            .field("address_snapshot_encrypted", &"<redacted>")
            .field("address_snapshot_fingerprint", &"<redacted>")
            .finish()
    }
}

impl PartialEq for Delivery {
    /// 全字段语义相等（`Debug` 脱敏不影响比较）。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.delivery_no == other.delivery_no
            && self.delivery_type == other.delivery_type
            && self.sales_order_id == other.sales_order_id
            && self.purchase_order_id == other.purchase_order_id
            && self.warehouse_id == other.warehouse_id
            && self.status == other.status
            && self.carrier == other.carrier
            && self.tracking_no == other.tracking_no
            && self.shipped_at == other.shipped_at
            && self.address_snapshot_encrypted == other.address_snapshot_encrypted
            && self.address_snapshot_fingerprint == other.address_snapshot_fingerprint
    }
}

impl Eq for Delivery {}

impl Delivery {
    /// 生成履约地址查询指纹。
    ///
    /// 对规范化后的履约地址原文字符串计算带密钥 HMAC-SHA256（§4.5.5，
    /// 禁止裸摘要）；密钥不持久化，精确查询时用同一密钥比对指纹。
    ///
    /// # 参数
    /// * `plain` - 履约地址原文字符串
    /// * `key` - 查询指纹密钥字节
    ///
    /// # 返回
    /// 返回 64 位小写十六进制指纹。
    pub fn address_snapshot_fingerprint(plain: &str, key: &[u8]) -> String {
        hmac_sha256_hex(key, plain.as_bytes())
    }

    /// 创建履约发货单（初始状态为草稿）。
    ///
    /// 完成单号/物流字段规范化，并按发货类型校验来源归属：
    /// 仓发必填 `warehouse_id` 且采购来源为空；供应商直发必填
    /// `purchase_order_id` 且仓库为空（§6.7）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::DeliveryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的发货单实体。
    ///
    /// # 错误
    /// 单号为空/超长、物流字段超长、仓发/直发归属与字典不一致，或指纹格式
    /// 非法时返回错误。
    pub fn new(id: DeliveryId, data: DeliveryData) -> Result<Self> {
        let delivery_no = normalize_required_text(
            data.delivery_no,
            "发货单号不能为空",
            DELIVERY_NO_MAX_LEN,
            "发货单号过长",
        )?;
        let carrier = normalize_optional_text(data.carrier, "物流承运方", CARRIER_MAX_LEN)?;
        let tracking_no = normalize_optional_text(data.tracking_no, "物流单号", TRACKING_NO_MAX_LEN)?;
        let address_snapshot_encrypted = normalize_optional_text(
            data.address_snapshot_encrypted,
            "履约地址加密值",
            ADDRESS_ENCRYPTED_MAX_LEN,
        )?;
        let address_snapshot_fingerprint = normalize_optional_text(
            data.address_snapshot_fingerprint,
            "履约地址查询指纹",
            crate::fulfillment::fingerprint::FINGERPRINT_HEX_LEN,
        )?;
        if let Some(fingerprint) = &address_snapshot_fingerprint {
            validate_fingerprint(fingerprint)?;
        }
        validate_source_ownership(data.delivery_type, &data.warehouse_id, &data.purchase_order_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            delivery_no,
            delivery_type: data.delivery_type,
            sales_order_id: data.sales_order_id,
            purchase_order_id: data.purchase_order_id,
            warehouse_id: data.warehouse_id,
            status: DeliveryState::Draft,
            carrier,
            tracking_no,
            shipped_at: None,
            address_snapshot_encrypted,
            address_snapshot_fingerprint,
        })
    }

    /// 返回单据注册与无审批绑定重验使用的组织上下文。
    ///
    /// # 返回
    /// 返回所属销售单稳定主键。
    ///
    /// # 错误
    /// 销售单主键为空时返回错误。
    pub fn registration_context_id(&self) -> Result<&str> {
        let id = self.sales_order_id.as_ref();
        if id.trim().is_empty() {
            return Err(Error::from("发货单缺少销售单，无法构造绑定上下文"));
        }
        Ok(id)
    }

    /// 校验发货行可作为指定验收行的履约事实并返回成功数量。
    ///
    /// # 参数
    /// * `line` - 已加载的发货行
    /// * `sales_order_id` - 验收单所属销售单
    /// * `sales_order_line_id` - 验收行所属销售稳定明细
    ///
    /// # 返回
    /// 状态与两级关联一致时返回发货行数量。
    ///
    /// # 错误
    /// 发货状态无效，或表头、发货行、销售单、销售明细关联不一致时返回错误。
    pub fn acceptance_quantity(
        &self,
        line: &DeliveryLine,
        sales_order_id: &SalesOrderId,
        sales_order_line_id: &SalesOrderLineId,
    ) -> Result<Quantity> {
        if !self.status.is_acceptance_eligible() || self.sales_order_id != *sales_order_id {
            return Err(Error::from("发货事实不属于本销售单或状态无效"));
        }
        if line.delivery_id.as_ref() != self.base.id.as_str() {
            return Err(Error::from("发货行与发货单关联不一致"));
        }
        if line.sales_order_line_id != *sales_order_line_id {
            return Err(Error::from("履约事实不属于本验收明细"));
        }
        Ok(line.quantity)
    }

    /// 更新履约发货单（仅草稿）。
    ///
    /// 复用 `new` 的物流字段规范化规则；发货类型与来源归属创建后不可修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不可编辑，或物流字段超长时返回错误。
    pub fn update(&mut self, update: DeliveryUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(carrier) = update.carrier {
            self.carrier = normalize_optional_text(Some(carrier), "物流承运方", CARRIER_MAX_LEN)?;
        }
        if let Some(tracking_no) = update.tracking_no {
            self.tracking_no = normalize_optional_text(Some(tracking_no), "物流单号", TRACKING_NO_MAX_LEN)?;
        }
        Ok(())
    }

    /// 登记发货（草稿 → 已发货）。
    ///
    /// 记录发货时间；仓发的预占消耗、出库流水与余额更新由 P3 在过账事务中
    /// 完成（§8.2 第 2 条）。
    ///
    /// # 参数
    /// * `shipped_at` - 发货时间
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非草稿）时返回错误。
    pub fn mark_shipped(&mut self, shipped_at: Instant) -> Result<()> {
        ensure_transition(self.status, DeliveryState::Shipped)?;
        self.shipped_at = Some(shipped_at);
        self.status = DeliveryState::Shipped;
        Ok(())
    }

    /// 登记签收（已发货 → 已签收）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非已发货）时返回错误。
    pub fn mark_signed(&mut self) -> Result<()> {
        ensure_transition(self.status, DeliveryState::Signed)?;
        self.status = DeliveryState::Signed;
        Ok(())
    }

    /// 冲正发货单（已发货或已签收 → 已冲正，终态）。
    ///
    /// `REVERSED` 表示存在正式反向事实（冲正出库流水/退货），不删除原事实
    /// （§4.5.1、§7.5）；反向事实由 P3 形成。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（草稿或已冲正）时返回错误。
    pub fn reverse(&mut self) -> Result<()> {
        ensure_transition(self.status, DeliveryState::Reversed)?;
        self.status = DeliveryState::Reversed;
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
    /// 已发货/已签收/已冲正不可编辑时返回错误。
    fn ensure_editable(&self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("已发货、已签收或已冲正的发货单不可编辑"));
        }
        Ok(())
    }
}

/// 发货行创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryLineData {
    /// 发货单。
    pub delivery_id: DeliveryId,
    /// 稳定行号（单内从 1 递增）。
    pub line_no: u32,
    /// 销售稳定明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 发货数量。
    pub quantity: Quantity,
    /// 仓发消耗的预占；直发为空。
    pub stock_reservation_id: Option<StockReservationId>,
    /// 供应商直发必填的采购到销售分配；仓发为空。
    pub purchase_line_sales_allocation_id: Option<PurchaseLineSalesAllocationId>,
}

/// 发货行实体（数据模型 §6.7 行）。
///
/// 行级归属按发货类型校验（§6.7）：仓发必填预占且直发分配为空；供应商直发
/// 必填采购到销售分配且预占为空。行级约束 `(delivery_id, line_no)` 唯一由
/// 唯一索引保证；仓发必须消耗本销售明细的有效预占并形成出库流水等跨聚合
/// 动作由 P3 完成（§8.2 第 2 条）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct DeliveryLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 发货单。
    pub delivery_id: DeliveryId,
    /// 稳定行号。
    pub line_no: u32,
    /// 销售稳定明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 发货数量。
    pub quantity: Quantity,
    /// 仓发消耗的预占。
    pub stock_reservation_id: Option<StockReservationId>,
    /// 供应商直发的采购到销售分配。
    pub purchase_line_sales_allocation_id: Option<PurchaseLineSalesAllocationId>,
}

impl DeliveryLine {
    /// 创建发货行。
    ///
    /// 完成行号与数量校验，并按发货类型校验行级归属（仓发/直发与表头一致，
    /// 见 [`DeliveryLineData`]）。行不可独立于表头状态变更，已发货事实不因
    /// 销售或采购变更被删除（§6.7），行变更由 P3 按表头状态把关。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::DeliveryLineId`）
    /// * `data` - 创建数据
    /// * `delivery_type` - 所属发货单的发货类型（决定行级归属校验）
    ///
    /// # 返回
    /// 返回新建的发货行实体。
    ///
    /// # 错误
    /// 行号小于 1、发货数量非正，或行级归属与发货类型不一致时返回错误。
    pub fn new(id: DeliveryLineId, data: DeliveryLineData, delivery_type: DeliveryType) -> Result<Self> {
        if data.line_no < 1 {
            return Err(Error::from("行号必须从 1 开始"));
        }
        if data.quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("发货数量必须为正数"));
        }
        match delivery_type {
            DeliveryType::WarehouseShip => {
                if data.stock_reservation_id.is_none() {
                    return Err(Error::from("仓发必须消耗本销售明细的有效预占"));
                }
                if data.purchase_line_sales_allocation_id.is_some() {
                    return Err(Error::from("仓发行不得携带采购到销售分配"));
                }
            }
            DeliveryType::SupplierDirect => {
                if data.purchase_line_sales_allocation_id.is_none() {
                    return Err(Error::from("供应商直发必须引用采购到销售分配"));
                }
                if data.stock_reservation_id.is_some() {
                    return Err(Error::from("供应商直发行不得携带库存预占"));
                }
            }
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            delivery_id: data.delivery_id,
            line_no: data.line_no,
            sales_order_line_id: data.sales_order_line_id,
            quantity: data.quantity,
            stock_reservation_id: data.stock_reservation_id,
            purchase_line_sales_allocation_id: data.purchase_line_sales_allocation_id,
        })
    }
}

/// 校验仓发/直发的表头来源归属。
///
/// # 参数
/// * `delivery_type` - 发货类型
/// * `warehouse_id` - 仓库
/// * `purchase_order_id` - 采购来源
///
/// # 返回
/// 通过返回 `Ok(())`。
///
/// # 错误
/// 仓发缺少仓库或携带采购来源、直发缺少采购来源或携带仓库时返回错误。
fn validate_source_ownership(
    delivery_type: DeliveryType,
    warehouse_id: &Option<WarehouseId>,
    purchase_order_id: &Option<PurchaseOrderId>,
) -> Result<()> {
    match delivery_type {
        DeliveryType::WarehouseShip => {
            if warehouse_id.is_none() {
                return Err(Error::from("仓发必填入库仓"));
            }
            if purchase_order_id.is_some() {
                return Err(Error::from("仓发不得携带供应商直发的采购来源"));
            }
        }
        DeliveryType::SupplierDirect => {
            if purchase_order_id.is_none() {
                return Err(Error::from("供应商直发必填采购来源"));
            }
            if warehouse_id.is_some() {
                return Err(Error::from("供应商直发不得携带入库仓"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PurchaseLineSalesAllocationId;
    use std::str::FromStr;

    const PLAINTEXT_ADDRESS: &str = "上海市浦东新区世纪大道100号 张三 13800000000";
    const FINGERPRINT_KEY: &[u8] = b"test-fingerprint-key";

    fn delivery_data() -> DeliveryData {
        DeliveryData {
            delivery_no: " DV-2026-001 ".to_string(),
            delivery_type: DeliveryType::WarehouseShip,
            sales_order_id: SalesOrderId::new("so-1"),
            purchase_order_id: None,
            warehouse_id: Some(WarehouseId::new("wh-1")),
            carrier: Some(" 顺丰 ".to_string()),
            tracking_no: Some(" SF-001 ".to_string()),
            address_snapshot_encrypted: Some("ciphertext-base64...".to_string()),
            address_snapshot_fingerprint: Some(Delivery::address_snapshot_fingerprint(
                PLAINTEXT_ADDRESS,
                FINGERPRINT_KEY,
            )),
        }
    }

    fn line_data() -> DeliveryLineData {
        DeliveryLineData {
            delivery_id: DeliveryId::new("delivery-1"),
            line_no: 1,
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            quantity: Quantity::from_str("3").unwrap(),
            stock_reservation_id: Some(StockReservationId::new("rsv-1")),
            purchase_line_sales_allocation_id: None,
        }
    }

    /// happy path：单号/物流字段规范化、仓发归属、指纹生成与状态机全链路。
    #[test]
    fn new_normalizes_fields_and_drives_state_machine() {
        let mut delivery = Delivery::new(DeliveryId::new("delivery-1"), delivery_data()).unwrap();
        assert_eq!(delivery.delivery_no, "DV-2026-001");
        assert_eq!(delivery.carrier.as_deref(), Some("顺丰"));
        assert_eq!(delivery.tracking_no.as_deref(), Some("SF-001"));
        assert_eq!(delivery.status, DeliveryState::Draft);

        delivery
            .mark_shipped(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert_eq!(delivery.status, DeliveryState::Shipped);
        delivery.mark_signed().unwrap();
        assert_eq!(delivery.status, DeliveryState::Signed);
        delivery.reverse().unwrap();
        assert_eq!(delivery.status, DeliveryState::Reversed);
    }

    /// 失败路径：必填空（单号空白）、超长、仓发/直发归属不一致。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = DeliveryData {
            delivery_no: "   ".to_string(),
            ..delivery_data()
        };
        assert!(Delivery::new(DeliveryId::new("d2"), blank_no).is_err());

        let overlong_tracking = DeliveryData {
            tracking_no: Some("t".repeat(129)),
            ..delivery_data()
        };
        assert!(Delivery::new(DeliveryId::new("d3"), overlong_tracking).is_err());

        let warehouse_ship_without_warehouse = DeliveryData {
            warehouse_id: None,
            ..delivery_data()
        };
        assert!(Delivery::new(DeliveryId::new("d4"), warehouse_ship_without_warehouse).is_err());

        let direct_with_warehouse = DeliveryData {
            delivery_type: DeliveryType::SupplierDirect,
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            warehouse_id: Some(WarehouseId::new("wh-1")),
            ..delivery_data()
        };
        assert!(Delivery::new(DeliveryId::new("d5"), direct_with_warehouse).is_err());

        let direct_without_po = DeliveryData {
            delivery_type: DeliveryType::SupplierDirect,
            purchase_order_id: None,
            ..delivery_data()
        };
        assert!(Delivery::new(DeliveryId::new("d6"), direct_without_po).is_err());

        let bad_fingerprint = DeliveryData {
            address_snapshot_fingerprint: Some("not-a-fingerprint".to_string()),
            ..delivery_data()
        };
        assert!(Delivery::new(DeliveryId::new("d7"), bad_fingerprint).is_err());
    }

    /// 状态机：草稿不可冲正、已发货不可重发、已冲正终态不可再迁移。
    #[test]
    fn state_machine_directed_edges() {
        let mut delivery = Delivery::new(DeliveryId::new("delivery-2"), delivery_data()).unwrap();
        assert!(delivery.reverse().is_err(), "草稿不能直接冲正");
        assert!(delivery.mark_signed().is_err(), "未发货不能签收");
        assert!(delivery
            .update(DeliveryUpdate {
                carrier: Some(" 京东 ".to_string()),
                tracking_no: None,
            })
            .is_ok());

        delivery
            .mark_shipped(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert!(
            delivery
                .update(DeliveryUpdate {
                    carrier: None,
                    tracking_no: None,
                })
                .is_err(),
            "已发货不可编辑"
        );
        // from == to 幂等迁移恒合法（state.rs 契约）；SHIPPED 不可编辑由 update 把关。
        assert!(delivery
            .mark_shipped(Instant::from_unix_secs(1_700_000_100))
            .is_ok());
        assert!(delivery.reverse().is_ok());
        assert!(delivery.mark_signed().is_err(), "REVERSED 是终态，不能签收");
    }

    /// 验收事实要求已发货状态且表头、行与销售归属一致。
    #[test]
    fn acceptance_fact_checks_status_and_associations() {
        let mut delivery = Delivery::new(DeliveryId::new("delivery-1"), delivery_data()).unwrap();
        let line = DeliveryLine::new(
            DeliveryLineId::new("delivery-line-1"),
            line_data(),
            DeliveryType::WarehouseShip,
        )
        .unwrap();
        assert!(delivery
            .acceptance_quantity(
                &line,
                &SalesOrderId::new("so-1"),
                &SalesOrderLineId::new("so-line-1"),
            )
            .is_err());
        delivery
            .mark_shipped(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert_eq!(
            delivery
                .acceptance_quantity(
                    &line,
                    &SalesOrderId::new("so-1"),
                    &SalesOrderLineId::new("so-line-1"),
                )
                .unwrap(),
            Quantity::from_str("3").unwrap()
        );
        assert!(delivery
            .acceptance_quantity(
                &line,
                &SalesOrderId::new("other-order"),
                &SalesOrderLineId::new("so-line-1"),
            )
            .is_err());
        let mut foreign_delivery_line = line.clone();
        foreign_delivery_line.delivery_id = DeliveryId::new("other-delivery");
        assert!(delivery
            .acceptance_quantity(
                &foreign_delivery_line,
                &SalesOrderId::new("so-1"),
                &SalesOrderLineId::new("so-line-1"),
            )
            .is_err());
        let mut foreign_sales_line = line.clone();
        foreign_sales_line.sales_order_line_id = SalesOrderLineId::new("other-line");
        assert!(delivery
            .acceptance_quantity(
                &foreign_sales_line,
                &SalesOrderId::new("so-1"),
                &SalesOrderLineId::new("so-line-1"),
            )
            .is_err());
        assert_eq!(delivery.registration_context_id().unwrap(), "so-1");
        let mut missing_context = delivery.clone();
        missing_context.sales_order_id = SalesOrderId::new("   ");
        assert!(missing_context.registration_context_id().is_err());
    }

    /// 状态机：固定邻接矩阵的合法/非法迁移（幂等合法）。
    #[test]
    fn state_machine_transition_matrix() {
        assert!(ensure_transition(DeliveryState::Draft, DeliveryState::Shipped).is_ok());
        assert!(ensure_transition(DeliveryState::Shipped, DeliveryState::Signed).is_ok());
        assert!(ensure_transition(DeliveryState::Shipped, DeliveryState::Reversed).is_ok());
        assert!(ensure_transition(DeliveryState::Signed, DeliveryState::Reversed).is_ok());
        assert!(ensure_transition(DeliveryState::Draft, DeliveryState::Signed).is_err());
        assert!(ensure_transition(DeliveryState::Draft, DeliveryState::Reversed).is_err());
        assert!(ensure_transition(DeliveryState::Signed, DeliveryState::Shipped).is_err());
        assert!(ensure_transition(DeliveryState::Reversed, DeliveryState::Shipped).is_err());
        assert!(ensure_transition(DeliveryState::Reversed, DeliveryState::Reversed).is_ok());
    }

    /// 敏感字段：指纹稳定且带密钥；Debug 不泄漏明文/密文/指纹。
    #[test]
    fn sensitive_address_fingerprint_and_redacted_debug() {
        let a = Delivery::address_snapshot_fingerprint(PLAINTEXT_ADDRESS, FINGERPRINT_KEY);
        let b = Delivery::address_snapshot_fingerprint(PLAINTEXT_ADDRESS, FINGERPRINT_KEY);
        assert_eq!(a, b, "同密钥同明文指纹稳定");
        assert_ne!(
            a,
            Delivery::address_snapshot_fingerprint(PLAINTEXT_ADDRESS, b"another-key"),
            "指纹带密钥"
        );
        assert_eq!(a.len(), 64, "HMAC-SHA256 十六进制为 64 位");

        let delivery = Delivery::new(DeliveryId::new("delivery-3"), delivery_data()).unwrap();
        let debug = format!("{delivery:?}");
        assert!(!debug.contains(PLAINTEXT_ADDRESS), "Debug 不得泄漏明文地址");
        assert!(!debug.contains("ciphertext-base64"), "Debug 不得泄漏密文");
        assert!(!debug.contains(&a), "Debug 不得泄漏指纹");
        assert!(debug.contains("<redacted>"));
    }

    /// happy path：仓发行创建成功；直发行创建成功。
    #[test]
    fn line_new_succeeds_for_both_types() {
        let warehouse_line = DeliveryLine::new(
            DeliveryLineId::new("dl-1"),
            line_data(),
            DeliveryType::WarehouseShip,
        )
        .unwrap();
        assert_eq!(
            warehouse_line.stock_reservation_id.as_ref().unwrap().as_ref(),
            "rsv-1"
        );

        let direct_line = DeliveryLine::new(
            DeliveryLineId::new("dl-2"),
            DeliveryLineData {
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("pla-1")),
                ..line_data()
            },
            DeliveryType::SupplierDirect,
        )
        .unwrap();
        assert_eq!(
            direct_line
                .purchase_line_sales_allocation_id
                .as_ref()
                .unwrap()
                .as_ref(),
            "pla-1"
        );
    }

    /// 失败路径：行级归属不一致与数量越界。
    #[test]
    fn line_rejects_ownership_and_quantity_violations() {
        let no_reservation = DeliveryLineData {
            stock_reservation_id: None,
            ..line_data()
        };
        assert!(DeliveryLine::new(
            DeliveryLineId::new("dl-3"),
            no_reservation,
            DeliveryType::WarehouseShip
        )
        .is_err());

        let direct_with_reservation = DeliveryLineData {
            purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("pla-1")),
            stock_reservation_id: Some(StockReservationId::new("rsv-1")),
            ..line_data()
        };
        assert!(DeliveryLine::new(
            DeliveryLineId::new("dl-4"),
            direct_with_reservation,
            DeliveryType::SupplierDirect
        )
        .is_err());

        let zero_quantity = DeliveryLineData {
            quantity: Quantity::from_str("0").unwrap(),
            ..line_data()
        };
        assert!(DeliveryLine::new(
            DeliveryLineId::new("dl-5"),
            zero_quantity,
            DeliveryType::WarehouseShip
        )
        .is_err());

        let zero_line_no = DeliveryLineData {
            line_no: 0,
            ..line_data()
        };
        assert!(DeliveryLine::new(
            DeliveryLineId::new("dl-6"),
            zero_line_no,
            DeliveryType::WarehouseShip
        )
        .is_err());
    }

    /// 序列化：枚举稳定代码；实体 BSON 往返（含密文与指纹字段）。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&DeliveryType::SupplierDirect).unwrap(),
            "\"SUPPLIER_DIRECT\""
        );
        assert_eq!(
            serde_json::to_string(&DeliveryState::Signed).unwrap(),
            "\"SIGNED\""
        );
        assert_eq!(DeliveryState::Shipped.label(), "已发货");

        let mut delivery = Delivery::new(DeliveryId::new("delivery-4"), delivery_data()).unwrap();
        delivery
            .mark_shipped(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        let roundtrip: Delivery =
            bson::deserialize_from_document(bson::serialize_to_document(&delivery).unwrap()).unwrap();
        assert_eq!(roundtrip, delivery);
    }

    /// 发货单无审批约束：不得出现绑定字段或审批状态机。
    #[test]
    fn delivery_has_no_approval_binding_or_state_machine() {
        let delivery = Delivery::new(DeliveryId::new("delivery-1"), delivery_data()).unwrap();
        let value = serde_json::to_value(&delivery).unwrap();
        let object = value.as_object().expect("发货单序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("pending_allocations"));
        assert_eq!(delivery.status, DeliveryState::Draft);
        assert_eq!(DeliveryState::Draft.as_str(), "DRAFT");
        assert_eq!(DeliveryState::Shipped.as_str(), "SHIPPED");
        assert_eq!(DeliveryState::Signed.as_str(), "SIGNED");
        assert_eq!(DeliveryState::Reversed.as_str(), "REVERSED");

        let production = include_str!("delivery.rs")
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
