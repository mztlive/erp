//! `stock_balance`：库存余额（数据模型 §6.7）。
//!
//! `(warehouse_id, sku_id)` 唯一；`available_quantity = on_hand_quantity -
//! reserved_quantity` 且三个数量均不得为负（§6.7、§8.2 第 4 条）在实体构造
//! 与更新时校验。`stock_movement` 是事实源，余额可从流水重建，但日常提交后
//! 必须立即可见——余额联动在 P3 过账事务中完成。`BaseModel.version` 即数据
//! 模型 `lock_version`（common/README），并发控制由 P0 基元承担。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{SkuId, StockBalanceId, StockMovementId, WarehouseId};
use crate::money::Quantity;

use super::stock_adjustment::{StockAdjustment, StockAdjustmentLine};

/// 库存余额创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockBalanceData {
    /// 库存维度：仓库。
    pub warehouse_id: WarehouseId,
    /// 库存维度：SKU。
    pub sku_id: SkuId,
    /// 账面现存。
    pub on_hand_quantity: Quantity,
    /// 有效预占。
    pub reserved_quantity: Quantity,
    /// 可用数量（必须等于 `on_hand - reserved`）。
    pub available_quantity: Quantity,
    /// 已应用最后流水。
    pub last_movement_id: Option<StockMovementId>,
}

/// 库存余额更新数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockBalanceUpdate {
    /// 账面现存；`None` 表示不修改。
    pub on_hand_quantity: Option<Quantity>,
    /// 有效预占；`None` 表示不修改。
    pub reserved_quantity: Option<Quantity>,
    /// 已应用最后流水；`None` 表示不修改。
    pub last_movement_id: Option<Option<StockMovementId>>,
}

/// 库存余额实体（数据模型 §6.7，事务内同步维护的当前余额）。
///
/// 三个数量均不得为负，且恒满足 `available = on_hand - reserved`（§8.2
/// 第 4 条）；更新时重算 `available_quantity`。余额行锁定/等价并发控制由
/// P3 过账事务完成。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct StockBalance {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 账面现存。
    pub on_hand_quantity: Quantity,
    /// 有效预占。
    pub reserved_quantity: Quantity,
    /// 可用数量。
    pub available_quantity: Quantity,
    /// 已应用最后流水。
    pub last_movement_id: Option<StockMovementId>,
}

impl StockBalance {
    /// 创建库存余额。
    ///
    /// 完成非负校验与三元组一致性校验：`available` 必须精确等于
    /// `on_hand - reserved`（§6.7/§8.2 第 4 条）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::StockBalanceId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的余额实体。
    ///
    /// # 错误
    /// 任一数量为负，或 `available` 不等于 `on_hand - reserved` 时返回错误。
    pub fn new(id: StockBalanceId, data: StockBalanceData) -> Result<Self> {
        ensure_quantities_non_negative(
            data.on_hand_quantity,
            data.reserved_quantity,
            data.available_quantity,
        )?;
        if data.available_quantity.to_decimal()
            != data.on_hand_quantity.to_decimal() - data.reserved_quantity.to_decimal()
        {
            return Err(Error::from("可用数量必须等于账面现存减去有效预占"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            warehouse_id: data.warehouse_id,
            sku_id: data.sku_id,
            on_hand_quantity: data.on_hand_quantity,
            reserved_quantity: data.reserved_quantity,
            available_quantity: data.available_quantity,
            last_movement_id: data.last_movement_id,
        })
    }
    /// 更新库存余额。
    ///
    /// 复用 `new` 的数量约束；`available_quantity` 由 `on_hand - reserved`
    /// 重算并校验非负（§8.2 第 4 条）。校验通过后才写入字段，失败不产生
    /// 部分更新。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 更新后任一数量为负（可用量不足）时返回错误。
    pub fn update(&mut self, update: StockBalanceUpdate) -> Result<()> {
        let on_hand = update.on_hand_quantity.unwrap_or(self.on_hand_quantity);
        let reserved = update.reserved_quantity.unwrap_or(self.reserved_quantity);
        let available = on_hand.to_decimal() - reserved.to_decimal();
        let zero = rust_decimal::Decimal::ZERO;
        if on_hand.to_decimal() < zero || reserved.to_decimal() < zero || available < zero {
            return Err(Error::from("账面现存、有效预占与可用数量均不得为负"));
        }
        self.on_hand_quantity = on_hand;
        self.reserved_quantity = reserved;
        self.available_quantity = Quantity::try_from(available).expect("可用数量小数位受 Quantity 约束");
        if let Some(last_movement_id) = update.last_movement_id {
            self.last_movement_id = last_movement_id;
        }
        Ok(())
    }

    /// 判断当前乐观锁版本是否与期望版本一致。
    ///
    /// # 参数
    /// * `expected` - 调用方读取后携带的期望版本
    ///
    /// # 返回
    /// 当前版本等于期望版本时返回 `true`。
    pub fn matches_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 判断库存余额与调整单及其明细是否属于同一库存维度。
    ///
    /// # 参数
    /// * `adjustment` - 待创建或提交的调整单
    /// * `lines` - 调整单全部明细
    ///
    /// # 返回
    /// 仓库一致且所有明细 SKU 都等于余额 SKU 时返回 `true`。
    pub fn matches_adjustment_dimensions(
        &self,
        adjustment: &StockAdjustment,
        lines: &[StockAdjustmentLine],
    ) -> bool {
        self.warehouse_id == adjustment.warehouse_id
            && lines.iter().all(|line| {
                line.stock_adjustment_id.as_ref() == adjustment.base.id.as_str() && line.sku_id == self.sku_id
            })
    }
}

/// 校验三个数量均非负。
///
/// # 参数
/// * `on_hand` - 账面现存
/// * `reserved` - 有效预占
/// * `available` - 可用数量
///
/// # 返回
/// 通过返回 `Ok(())`。
///
/// # 错误
/// 任一数量为负时返回错误。
fn ensure_quantities_non_negative(on_hand: Quantity, reserved: Quantity, available: Quantity) -> Result<()> {
    let zero = rust_decimal::Decimal::ZERO;
    if on_hand.to_decimal() < zero || reserved.to_decimal() < zero || available.to_decimal() < zero {
        return Err(Error::from("账面现存、有效预占与可用数量均不得为负"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{StockAdjustmentId, StockBalanceId};
    use crate::money::Quantity;
    use std::str::FromStr;

    fn data() -> StockBalanceData {
        StockBalanceData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            on_hand_quantity: Quantity::from_str("100").unwrap(),
            reserved_quantity: Quantity::from_str("30").unwrap(),
            available_quantity: Quantity::from_str("70").unwrap(),
            last_movement_id: Some(StockMovementId::new("m-1")),
        }
    }

    /// happy path：三元组一致创建成功。
    #[test]
    fn new_succeeds_with_consistent_triple() {
        let balance = StockBalance::new(StockBalanceId::new("b-1"), data()).unwrap();
        assert_eq!(balance.available_quantity, Quantity::from_str("70").unwrap());
        assert_eq!(balance.last_movement_id.unwrap().as_ref(), "m-1");
    }

    /// 失败路径：三元组不一致与负数量（关联不一致）。
    #[test]
    fn new_rejects_inconsistent_or_negative_quantities() {
        let inconsistent = StockBalanceData {
            available_quantity: Quantity::from_str("69").unwrap(),
            ..data()
        };
        assert!(StockBalance::new(StockBalanceId::new("b-2"), inconsistent).is_err());

        let negative = StockBalanceData {
            reserved_quantity: Quantity::from_str("-1").unwrap(),
            available_quantity: Quantity::from_str("101").unwrap(),
            ..data()
        };
        assert!(StockBalance::new(StockBalanceId::new("b-3"), negative).is_err());

        let negative_available = StockBalanceData {
            reserved_quantity: Quantity::from_str("120").unwrap(),
            available_quantity: Quantity::from_str("-20").unwrap(),
            ..data()
        };
        assert!(StockBalance::new(StockBalanceId::new("b-4"), negative_available).is_err());
    }

    /// 更新：重算可用数量；超预占更新被拒。
    #[test]
    fn update_recomputes_available_and_guards_non_negative() {
        let mut balance = StockBalance::new(StockBalanceId::new("b-5"), data()).unwrap();
        balance
            .update(StockBalanceUpdate {
                on_hand_quantity: Some(Quantity::from_str("50").unwrap()),
                reserved_quantity: None,
                last_movement_id: None,
            })
            .unwrap();
        assert_eq!(balance.available_quantity, Quantity::from_str("20").unwrap());
        assert_eq!(balance.on_hand_quantity, Quantity::from_str("50").unwrap());

        let over_reserved = StockBalanceUpdate {
            reserved_quantity: Some(Quantity::from_str("80").unwrap()),
            ..StockBalanceUpdate::default()
        };
        assert!(balance.update(over_reserved).is_err(), "可用量不得为负");
        assert_eq!(
            balance.reserved_quantity,
            Quantity::from_str("30").unwrap(),
            "失败不改变字段"
        );
    }

    /// 版本与调整维度匹配由余额实体统一判定。
    #[test]
    fn version_and_adjustment_dimensions_are_matched() {
        let mut balance = StockBalance::new(StockBalanceId::new("b-6"), data()).unwrap();
        balance.base.version = 4;
        assert!(balance.matches_version(4));
        assert!(!balance.matches_version(3));

        let adjustment = StockAdjustment::new(
            StockAdjustmentId::new("adj-1"),
            crate::inventory::StockAdjustmentData {
                adjustment_no: "ADJ-1".to_string(),
                warehouse_id: WarehouseId::new("wh-1"),
                reason_type: crate::inventory::AdjustmentReasonType::StockLoss,
                prepared_by: "operator-1".to_string(),
                note: None,
                occurred_at: None,
            },
            "creator-1",
        )
        .unwrap();
        let line = StockAdjustmentLine::new_for_reason(
            crate::ids::StockAdjustmentLineId::new("line-1"),
            adjustment.reason_type,
            crate::inventory::StockAdjustmentLineData {
                stock_adjustment_id: StockAdjustmentId::new("adj-1"),
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("1").unwrap(),
                direction: crate::inventory::MovementDirection::Decrease,
            },
        )
        .unwrap();
        assert!(balance.matches_adjustment_dimensions(&adjustment, std::slice::from_ref(&line)));

        balance.sku_id = SkuId::new("sku-other");
        assert!(!balance.matches_adjustment_dimensions(&adjustment, &[line]));
    }

    /// 序列化：实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        let balance = StockBalance::new(StockBalanceId::new("b-6"), data()).unwrap();
        let roundtrip: StockBalance =
            bson::deserialize_from_document(bson::serialize_to_document(&balance).unwrap()).unwrap();
        assert_eq!(roundtrip, balance);
    }
}
