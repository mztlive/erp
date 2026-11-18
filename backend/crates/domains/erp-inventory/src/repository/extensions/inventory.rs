//! 域 D17 `inventory` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as InventoryExt>::STOCK_MOVEMENTS` 等值。

use mongodb::Database;

use super::super::inventory::{
    InventoryRepository, StockAdjustmentFilter, StockBalanceFilter, StockMovementFilter,
    StockReservationFilter,
};
use crate::entity::inventory::StockReservationEntry;
use crate::repository::owned::{
    StockAdjustmentLineRepository, StockAdjustmentRepository, StockBalanceRepository,
    StockMovementRepository, StockReservationRepository,
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

    /// 取得 `stock_movement` 集合的仓储，供库存增减流水查询与登记；由调用方提供执行器。
    fn stock_movements(&self) -> StockMovementRepository<'_>;

    /// 取得 `stock_balance` 集合的仓储，供按仓库和 SKU 读取与条件更新库存余额；由调用方提供执行器。
    fn stock_balances(&self) -> StockBalanceRepository<'_>;

    /// 取得 `stock_reservation` 集合的仓储，供库存预占查询、消耗和释放；由调用方提供执行器。
    fn stock_reservations(&self) -> StockReservationRepository<'_>;

    /// 取得 `stock_reservation_entry` 集合的仓储，供预占流水登记和读取；由调用方提供执行器。
    fn stock_reservation_entries(&self) -> persistence_core::Repository<'_, StockReservationEntry>;

    /// 取得 `stock_adjustment` 集合的仓储，供库存调整表头的查询与状态保存；由调用方提供执行器。
    fn stock_adjustments(&self) -> StockAdjustmentRepository<'_>;

    /// 取得 `stock_adjustment_line` 集合的仓储，供库存调整明细读写；由调用方提供执行器。
    fn stock_adjustment_lines(&self) -> StockAdjustmentLineRepository<'_>;

    /// 取得库存余额、流水、预占及调整单的跨集合操作入口；原子写入沿用用例事务，不在仓储内开启事务。
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
