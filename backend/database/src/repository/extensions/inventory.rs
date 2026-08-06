//! 域 D17 `inventory` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as InventoryExt>::STOCK_MOVEMENTS` 等值。

use entities::inventory::{
    StockAdjustment, StockAdjustmentLine, StockBalance, StockMovement, StockReservation,
    StockReservationEntry,
};
use mongodb::Database;

use super::super::inventory::{
    InventoryRepository, StockAdjustmentFilter, StockBalanceFilter, StockMovementFilter,
    StockReservationFilter,
};
use crate::Repository;

/// 域 D17 仓储访问器。
pub trait InventoryExt {
    /// `stock_movement` 集合名。
    const STOCK_MOVEMENTS: &'static str = "stock_movements";
    /// `stock_balance` 集合名。
    const STOCK_BALANCES: &'static str = "stock_balances";
    /// `stock_reservation` 集合名。
    const STOCK_RESERVATIONS: &'static str = "stock_reservations";
    /// `stock_reservation_entry` 集合名。
    const STOCK_RESERVATION_ENTRIES: &'static str = "stock_reservation_entries";
    /// `stock_adjustment` 集合名。
    const STOCK_ADJUSTMENTS: &'static str = "stock_adjustments";
    /// `stock_adjustment_line` 集合名。
    const STOCK_ADJUSTMENT_LINES: &'static str = "stock_adjustment_lines";

    /// 库存流水列表筛选条件类型（定义见 `repository::inventory`）。
    type StockMovementFilter;

    /// 库存余额列表筛选条件类型（定义见 `repository::inventory`）。
    type StockBalanceFilter;

    /// 库存预占列表筛选条件类型（定义见 `repository::inventory`）。
    type StockReservationFilter;

    /// 库存调整单列表筛选条件类型（定义见 `repository::inventory`）。
    type StockAdjustmentFilter;

    /// 获取 `stock_movement` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::inventory::StockMovement>`。
    fn stock_movements(&self) -> Repository<'_, StockMovement>;

    /// 获取 `stock_balance` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::inventory::StockBalance>`。
    fn stock_balances(&self) -> Repository<'_, StockBalance>;

    /// 获取 `stock_reservation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::inventory::StockReservation>`。
    fn stock_reservations(&self) -> Repository<'_, StockReservation>;

    /// 获取 `stock_reservation_entry` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::inventory::StockReservationEntry>`。
    fn stock_reservation_entries(&self) -> Repository<'_, StockReservationEntry>;

    /// 获取 `stock_adjustment` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::inventory::StockAdjustment>`。
    fn stock_adjustments(&self) -> Repository<'_, StockAdjustment>;

    /// 获取 `stock_adjustment_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::inventory::StockAdjustmentLine>`。
    fn stock_adjustment_lines(&self) -> Repository<'_, StockAdjustmentLine>;

    /// 获取承载跨集合写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `InventoryRepository` 实例。
    fn inventory(&self) -> InventoryRepository<'_>;
}

impl InventoryExt for Database {
    type StockMovementFilter = StockMovementFilter;
    type StockBalanceFilter = StockBalanceFilter;
    type StockReservationFilter = StockReservationFilter;
    type StockAdjustmentFilter = StockAdjustmentFilter;

    fn stock_movements(&self) -> Repository<'_, StockMovement> {
        Repository::new(self, Self::STOCK_MOVEMENTS)
    }

    fn stock_balances(&self) -> Repository<'_, StockBalance> {
        Repository::new(self, Self::STOCK_BALANCES)
    }

    fn stock_reservations(&self) -> Repository<'_, StockReservation> {
        Repository::new(self, Self::STOCK_RESERVATIONS)
    }

    fn stock_reservation_entries(&self) -> Repository<'_, StockReservationEntry> {
        Repository::new(self, Self::STOCK_RESERVATION_ENTRIES)
    }

    fn stock_adjustments(&self) -> Repository<'_, StockAdjustment> {
        Repository::new(self, Self::STOCK_ADJUSTMENTS)
    }

    fn stock_adjustment_lines(&self) -> Repository<'_, StockAdjustmentLine> {
        Repository::new(self, Self::STOCK_ADJUSTMENT_LINES)
    }

    fn inventory(&self) -> InventoryRepository<'_> {
        InventoryRepository::new(self)
    }
}
