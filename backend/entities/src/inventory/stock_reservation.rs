//! `stock_reservation` / `stock_reservation_entry`：库存预占及预占流水
//! （数据模型 §6.7）。
//!
//! 预占不设自动过期；只有审核生效的销售变更、销售单作废、采购退货、库存调整
//! 或仓发消耗可以改变预占，其他销售单不得消耗本预占（§6.7）——跨聚合动作由
//! P3 完成。`reserved_quantity + consumed_quantity + released_quantity` 不得
//! 超过原建立数量需要期初建立数量字段（字典未定义），由 P3 对照建立流水校验。
//! `stock_reservation_entry` 是预占流水，按字典建模（无事实时间字段），不提供
//! `update`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{
    PurchaseLineSalesAllocationId, PurchaseReceiptLineId, SalesOrderLineId, SkuId, StockReservationEntryId,
    StockReservationId, WarehouseId,
};
use crate::money::Quantity;
use crate::validation::normalize_required_text;

/// 来源单据标识最大长度。
const SOURCE_DOCUMENT_MAX_LEN: usize = 256;

/// 预占状态（数据模型 §6.7：有效、部分消耗、已消耗、已释放）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReservationStatus {
    /// 有效：已建立未消耗未释放。
    Active,
    /// 部分消耗：已消耗一部分，仍有剩余预占。
    PartiallyConsumed,
    /// 已消耗：全部消耗。
    Consumed,
    /// 已释放。
    Released,
}

impl ReservationStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "有效",
            Self::PartiallyConsumed => "部分消耗",
            Self::Consumed => "已消耗",
            Self::Released => "已释放",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::PartiallyConsumed => "PARTIALLY_CONSUMED",
            Self::Consumed => "CONSUMED",
            Self::Released => "RELEASED",
        }
    }

    /// 返回仍允许消耗或释放的预占状态集合。
    ///
    /// # 返回
    /// 返回有效与部分消耗两个可操作状态。
    pub fn operable() -> &'static [Self] {
        &[Self::Active, Self::PartiallyConsumed]
    }

    /// 判断当前预占状态是否仍可操作。
    ///
    /// # 返回
    /// 有效或部分消耗状态返回 `true`。
    pub fn is_operable(self) -> bool {
        Self::operable().contains(&self)
    }
}

/// 预占流水类型（数据模型 §6.7：建立、消耗、释放、冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReservationEntryType {
    /// 建立。
    Establish,
    /// 消耗（仓发）。
    Consume,
    /// 释放（变更/作废/退货/调整）。
    Release,
    /// 冲正。
    Reverse,
}

impl ReservationEntryType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Establish => "建立",
            Self::Consume => "消耗",
            Self::Release => "释放",
            Self::Reverse => "冲正",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Establish => "ESTABLISH",
            Self::Consume => "CONSUME",
            Self::Release => "RELEASE",
            Self::Reverse => "REVERSE",
        }
    }
}

/// 库存预占创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockReservationData {
    /// 预占库存：仓库。
    pub warehouse_id: WarehouseId,
    /// 预占库存：SKU。
    pub sku_id: SkuId,
    /// 唯一归属销售明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 来源采购分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 合格入库来源。
    pub source_receipt_line_id: PurchaseReceiptLineId,
    /// 当前有效预占。
    pub reserved_quantity: Quantity,
    /// 已消耗数量。
    pub consumed_quantity: Quantity,
    /// 已释放数量。
    pub released_quantity: Quantity,
    /// 预占状态。
    pub status: ReservationStatus,
}

/// 库存预占更新数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockReservationUpdate {
    /// 当前有效预占；`None` 表示不修改。
    pub reserved_quantity: Option<Quantity>,
    /// 已消耗数量；`None` 表示不修改。
    pub consumed_quantity: Option<Quantity>,
    /// 已释放数量；`None` 表示不修改。
    pub released_quantity: Option<Quantity>,
    /// 预占状态；`None` 表示不修改。
    pub status: Option<ReservationStatus>,
}

/// 库存预占实体（数据模型 §6.7）。
///
/// 预占只对原销售明细有效；改变预占的动作由 P3 按审核生效单据把关（§6.7）。
/// 状态与数量保持一致（见 [`StockReservation::new`] 的状态一致性校验）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct StockReservation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 预占库存：仓库。
    pub warehouse_id: WarehouseId,
    /// 预占库存：SKU。
    pub sku_id: SkuId,
    /// 唯一归属销售明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 来源采购分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 合格入库来源。
    pub source_receipt_line_id: PurchaseReceiptLineId,
    /// 当前有效预占。
    pub reserved_quantity: Quantity,
    /// 已消耗数量。
    pub consumed_quantity: Quantity,
    /// 已释放数量。
    pub released_quantity: Quantity,
    /// 预占状态。
    pub status: ReservationStatus,
}

impl StockReservation {
    /// 创建库存预占。
    ///
    /// 完成三个数量非负校验与状态一致性校验：`RELEASED` 必须有释放数量且无
    /// 剩余预占；`CONSUMED` 必须无剩余预占且无释放；`PARTIALLY_CONSUMED` 必须
    /// 有消耗且无释放；`ACTIVE` 必须无消耗无释放。预占建立动作与合格入库来源
    /// 的唯一性由唯一索引保证；累计不得超原建立数量由 P3 对照建立流水校验
    /// （§6.7）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::StockReservationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的预占实体。
    ///
    /// # 错误
    /// 数量为负或状态与数量不一致时返回错误。
    pub fn new(id: StockReservationId, data: StockReservationData) -> Result<Self> {
        ensure_quantities_valid(
            data.reserved_quantity,
            data.consumed_quantity,
            data.released_quantity,
        )?;
        ensure_status_coherent(
            data.status,
            data.reserved_quantity,
            data.consumed_quantity,
            data.released_quantity,
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            warehouse_id: data.warehouse_id,
            sku_id: data.sku_id,
            sales_order_line_id: data.sales_order_line_id,
            purchase_line_sales_allocation_id: data.purchase_line_sales_allocation_id,
            source_receipt_line_id: data.source_receipt_line_id,
            reserved_quantity: data.reserved_quantity,
            consumed_quantity: data.consumed_quantity,
            released_quantity: data.released_quantity,
            status: data.status,
        })
    }

    /// 更新库存预占。
    ///
    /// 复用 `new` 的数量与状态一致性校验；预占变化只能由审核生效的销售变更、
    /// 销售单作废、采购退货、库存调整或仓发消耗触发（§6.7，跨聚合动作由
    /// P3 把关）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 更新后数量为负或状态与数量不一致时返回错误。
    pub fn update(&mut self, update: StockReservationUpdate) -> Result<()> {
        let reserved = update.reserved_quantity.unwrap_or(self.reserved_quantity);
        let consumed = update.consumed_quantity.unwrap_or(self.consumed_quantity);
        let released = update.released_quantity.unwrap_or(self.released_quantity);
        let status = update.status.unwrap_or(self.status);
        ensure_quantities_valid(reserved, consumed, released)?;
        ensure_status_coherent(status, reserved, consumed, released)?;
        self.reserved_quantity = reserved;
        self.consumed_quantity = consumed;
        self.released_quantity = released;
        self.status = status;
        Ok(())
    }
}

/// 校验预占三个数量均非负。
///
/// # 参数
/// * `reserved` - 当前有效预占
/// * `consumed` - 已消耗数量
/// * `released` - 已释放数量
///
/// # 返回
/// 通过返回 `Ok(())`。
///
/// # 错误
/// 任一数量为负时返回错误。
fn ensure_quantities_valid(reserved: Quantity, consumed: Quantity, released: Quantity) -> Result<()> {
    let zero = rust_decimal::Decimal::ZERO;
    if reserved.to_decimal() < zero || consumed.to_decimal() < zero || released.to_decimal() < zero {
        return Err(Error::from("预占、消耗与释放数量均不得为负"));
    }
    Ok(())
}

/// 校验预占状态与数量的一致性。
///
/// # 参数
/// * `status` - 预占状态
/// * `reserved` - 当前有效预占
/// * `consumed` - 已消耗数量
/// * `released` - 已释放数量
///
/// # 返回
/// 通过返回 `Ok(())`。
///
/// # 错误
/// 状态与数量不一致时返回错误。
fn ensure_status_coherent(
    status: ReservationStatus,
    reserved: Quantity,
    consumed: Quantity,
    released: Quantity,
) -> Result<()> {
    let zero = rust_decimal::Decimal::ZERO;
    let consumed_positive = consumed.to_decimal() > zero;
    let released_positive = released.to_decimal() > zero;
    let reserved_positive = reserved.to_decimal() > zero;
    let coherent = match status {
        ReservationStatus::Active => reserved_positive && !consumed_positive && !released_positive,
        ReservationStatus::PartiallyConsumed => reserved_positive && consumed_positive && !released_positive,
        ReservationStatus::Consumed => !reserved_positive && consumed_positive && !released_positive,
        ReservationStatus::Released => !reserved_positive && released_positive,
    };
    if !coherent {
        return Err(Error::from("预占状态与数量不一致"));
    }
    Ok(())
}

/// 预占流水创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockReservationEntryData {
    /// 预占。
    pub reservation_id: StockReservationId,
    /// 流水类型。
    pub entry_type: ReservationEntryType,
    /// 正数数量。
    pub quantity: Quantity,
    /// 入库、仓发、销售变更、销售作废、采购退货或库存调整来源单据。
    pub source_document_id: String,
}

/// 预占流水实体（数据模型 §6.7，正式流水，不可变）。
///
/// 按字典精确建模（不含事实时间字段，只组合 `BaseModel`）；流水是正式事实，
/// 不设业务软删除且不提供 `update`（§4.5.1）。每个合格入库来源和采购分配的
/// 预占建立动作唯一由唯一索引保证。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct StockReservationEntry {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 预占。
    pub reservation_id: StockReservationId,
    /// 流水类型。
    pub entry_type: ReservationEntryType,
    /// 正数数量。
    pub quantity: Quantity,
    /// 来源单据。
    pub source_document_id: String,
}

impl StockReservationEntry {
    /// 创建预占流水。
    ///
    /// 完成数量正数校验与来源单据规范化。`source_document_id` 是入库、仓发、
    /// 销售变更、销售作废、采购退货或库存调整等单据的跨域多态引用，无统一
    /// ID newtype，以字符串承载（地基修订候选：公共的 `SourceDocumentRef`
    /// 值对象），P3 按单据类型解析并校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::StockReservationEntryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的预占流水实体。
    ///
    /// # 错误
    /// 数量非正或来源单据为空/超长时返回错误。
    pub fn new(id: StockReservationEntryId, data: StockReservationEntryData) -> Result<Self> {
        if data.quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("预占流水数量必须为正数"));
        }
        let source_document_id = normalize_required_text(
            data.source_document_id,
            "来源单据不能为空",
            SOURCE_DOCUMENT_MAX_LEN,
            "来源单据过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            reservation_id: data.reservation_id,
            entry_type: data.entry_type,
            quantity: data.quantity,
            source_document_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{StockReservationEntryId, StockReservationId};
    use std::str::FromStr;

    fn data() -> StockReservationData {
        StockReservationData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            source_receipt_line_id: PurchaseReceiptLineId::new("receipt-line-1"),
            reserved_quantity: Quantity::from_str("10").unwrap(),
            consumed_quantity: Quantity::from_str("0").unwrap(),
            released_quantity: Quantity::from_str("0").unwrap(),
            status: ReservationStatus::Active,
        }
    }

    fn entry_data() -> StockReservationEntryData {
        StockReservationEntryData {
            reservation_id: StockReservationId::new("rsv-1"),
            entry_type: ReservationEntryType::Establish,
            quantity: Quantity::from_str("10").unwrap(),
            source_document_id: " receipt-line-1 ".to_string(),
        }
    }

    /// happy path：预占创建成功，数量与状态一致。
    #[test]
    fn new_succeeds_with_coherent_state() {
        let reservation = StockReservation::new(StockReservationId::new("rsv-1"), data()).unwrap();
        assert_eq!(reservation.reserved_quantity, Quantity::from_str("10").unwrap());
        assert_eq!(reservation.status, ReservationStatus::Active);
    }

    /// 状态一致性：全部状态 × 数量组合的合法与非法判定。
    #[test]
    fn status_coherence_is_enforced() {
        let partially_consumed = StockReservationData {
            reserved_quantity: Quantity::from_str("7").unwrap(),
            consumed_quantity: Quantity::from_str("3").unwrap(),
            status: ReservationStatus::PartiallyConsumed,
            ..data()
        };
        assert!(StockReservation::new(StockReservationId::new("r2"), partially_consumed).is_ok());

        let consumed = StockReservationData {
            reserved_quantity: Quantity::from_str("0").unwrap(),
            consumed_quantity: Quantity::from_str("10").unwrap(),
            status: ReservationStatus::Consumed,
            ..data()
        };
        assert!(StockReservation::new(StockReservationId::new("r3"), consumed).is_ok());

        let released = StockReservationData {
            reserved_quantity: Quantity::from_str("0").unwrap(),
            released_quantity: Quantity::from_str("10").unwrap(),
            status: ReservationStatus::Released,
            ..data()
        };
        assert!(StockReservation::new(StockReservationId::new("r4"), released).is_ok());

        let active_with_consumption = StockReservationData {
            consumed_quantity: Quantity::from_str("1").unwrap(),
            ..data()
        };
        assert!(StockReservation::new(StockReservationId::new("r5"), active_with_consumption).is_err());

        let consumed_with_reserved = StockReservationData {
            consumed_quantity: Quantity::from_str("3").unwrap(),
            status: ReservationStatus::Consumed,
            ..data()
        };
        assert!(StockReservation::new(StockReservationId::new("r6"), consumed_with_reserved).is_err());

        let released_with_consumed = StockReservationData {
            reserved_quantity: Quantity::from_str("0").unwrap(),
            consumed_quantity: Quantity::from_str("4").unwrap(),
            released_quantity: Quantity::from_str("6").unwrap(),
            status: ReservationStatus::Released,
            ..data()
        };
        assert!(
            StockReservation::new(StockReservationId::new("r7"), released_with_consumed).is_ok(),
            "部分消耗后整体释放：释放状态允许存在已消耗数量"
        );
    }

    /// 失败路径：负数量。
    #[test]
    fn new_rejects_negative_quantities() {
        let negative = StockReservationData {
            consumed_quantity: Quantity::from_str("-1").unwrap(),
            ..data()
        };
        assert!(StockReservation::new(StockReservationId::new("r8"), negative).is_err());
    }

    /// 更新：数量与状态同步变化，不一致被拒。
    #[test]
    fn update_keeps_coherence() {
        let mut reservation = StockReservation::new(StockReservationId::new("rsv-9"), data()).unwrap();
        reservation
            .update(StockReservationUpdate {
                reserved_quantity: Some(Quantity::from_str("7").unwrap()),
                consumed_quantity: Some(Quantity::from_str("3").unwrap()),
                status: Some(ReservationStatus::PartiallyConsumed),
                released_quantity: None,
            })
            .unwrap();
        assert_eq!(reservation.status, ReservationStatus::PartiallyConsumed);

        assert!(
            reservation
                .update(StockReservationUpdate {
                    status: Some(ReservationStatus::Active),
                    ..StockReservationUpdate::default()
                })
                .is_err(),
            "有消耗时不能回到有效状态"
        );
        assert_eq!(
            reservation.status,
            ReservationStatus::PartiallyConsumed,
            "失败更新不得破坏原状态"
        );
    }

    /// 状态可操作性：只有有效与部分消耗允许继续消耗或释放。
    #[test]
    fn operable_statuses_are_owned_by_status_type() {
        assert_eq!(
            ReservationStatus::operable(),
            &[ReservationStatus::Active, ReservationStatus::PartiallyConsumed]
        );
        assert!(ReservationStatus::Active.is_operable());
        assert!(ReservationStatus::PartiallyConsumed.is_operable());
        assert!(!ReservationStatus::Consumed.is_operable());
        assert!(!ReservationStatus::Released.is_operable());
    }

    /// happy path：预占流水创建成功并规范化来源。
    #[test]
    fn entry_new_succeeds() {
        let entry =
            StockReservationEntry::new(StockReservationEntryId::new("entry-1"), entry_data()).unwrap();
        assert_eq!(entry.source_document_id, "receipt-line-1");
        assert_eq!(entry.entry_type, ReservationEntryType::Establish);
    }

    /// 失败路径：数量非正、来源为空。
    #[test]
    fn entry_rejects_invalid_inputs() {
        let zero_quantity = StockReservationEntryData {
            quantity: Quantity::from_str("0").unwrap(),
            ..entry_data()
        };
        assert!(StockReservationEntry::new(StockReservationEntryId::new("e2"), zero_quantity).is_err());

        let blank_source = StockReservationEntryData {
            source_document_id: "   ".to_string(),
            ..entry_data()
        };
        assert!(StockReservationEntry::new(StockReservationEntryId::new("e3"), blank_source).is_err());

        let overlong_source = StockReservationEntryData {
            source_document_id: "x".repeat(257),
            ..entry_data()
        };
        assert!(StockReservationEntry::new(StockReservationEntryId::new("e4"), overlong_source).is_err());
    }

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&ReservationStatus::PartiallyConsumed).unwrap(),
            "\"PARTIALLY_CONSUMED\""
        );
        assert_eq!(
            serde_json::to_string(&ReservationEntryType::Release).unwrap(),
            "\"RELEASE\""
        );
        assert_eq!(ReservationStatus::Released.label(), "已释放");

        let reservation = StockReservation::new(StockReservationId::new("rsv-10"), data()).unwrap();
        let roundtrip: StockReservation =
            bson::deserialize_from_document(bson::serialize_to_document(&reservation).unwrap()).unwrap();
        assert_eq!(roundtrip, reservation);
    }
}
