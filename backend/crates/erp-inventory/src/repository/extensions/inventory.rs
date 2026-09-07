//! 域 D17 `inventory` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as InventoryExt>::STOCK_MOVEMENTS` 等值。

use crate::entity::inventory::StockReservationEntry;
use crate::repository::owned::{
    StockAdjustmentLineRepository, StockAdjustmentRepository, StockBalanceRepository,
    StockMovementRepository, StockReservationRepository,
};
use mongodb::Database;

use super::super::inventory::{
    InventoryRepository, StockAdjustmentFilter, StockBalanceFilter, StockMovementFilter,
    StockReservationFilter,
};

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
    /// 返回 `StockMovementRepository<'_>`。
    fn stock_movements(&self) -> StockMovementRepository<'_>;

    /// 获取 `stock_balance` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `StockBalanceRepository<'_>`。
    fn stock_balances(&self) -> StockBalanceRepository<'_>;

    /// 获取 `stock_reservation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `StockReservationRepository<'_>`。
    fn stock_reservations(&self) -> StockReservationRepository<'_>;

    /// 获取 `stock_reservation_entry` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `persistence_core::Repository<'_, crate::entity::inventory::StockReservationEntry>`。
    fn stock_reservation_entries(&self) -> persistence_core::Repository<'_, StockReservationEntry>;

    /// 获取 `stock_adjustment` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `StockAdjustmentRepository<'_>`。
    fn stock_adjustments(&self) -> StockAdjustmentRepository<'_>;

    /// 获取 `stock_adjustment_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `StockAdjustmentLineRepository<'_>`。
    fn stock_adjustment_lines(&self) -> StockAdjustmentLineRepository<'_>;

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

    fn stock_movements(&self) -> StockMovementRepository<'_> {
        StockMovementRepository::new(self, Self::STOCK_MOVEMENTS)
    }

    fn stock_balances(&self) -> StockBalanceRepository<'_> {
        StockBalanceRepository::new(self, Self::STOCK_BALANCES)
    }

    fn stock_reservations(&self) -> StockReservationRepository<'_> {
        StockReservationRepository::new(self, Self::STOCK_RESERVATIONS)
    }

    fn stock_reservation_entries(&self) -> persistence_core::Repository<'_, StockReservationEntry> {
        persistence_core::Repository::new(self, Self::STOCK_RESERVATION_ENTRIES)
    }

    fn stock_adjustments(&self) -> StockAdjustmentRepository<'_> {
        StockAdjustmentRepository::new(self, Self::STOCK_ADJUSTMENTS)
    }

    fn stock_adjustment_lines(&self) -> StockAdjustmentLineRepository<'_> {
        StockAdjustmentLineRepository::new(self, Self::STOCK_ADJUSTMENT_LINES)
    }

    fn inventory(&self) -> InventoryRepository<'_> {
        InventoryRepository::new(self)
    }
}
