//! 域 D17 `inventory` 仓储：按库存集合能力与跨域聚合读取拆分。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`super::Repository`] 基类；库存余额与预占热点写入
//! 保持原子条件更新，事务边界与执行器选择仍由 Service 负责。

mod adjustment;
mod aggregate;
mod balance;
mod movement;
mod reservation;
mod shared;

use mongodb::Database;

use super::extensions::{CatalogExt, DocumentRegistryExt, FulfillmentExt, InventoryExt, WarehouseExt};

#[allow(unused_imports)]
pub use adjustment::{StockAdjustmentFilter, StockAdjustmentRow};
#[allow(unused_imports)]
pub use balance::{StockBalanceFilter, StockBalanceRow};
#[allow(unused_imports)]
pub use movement::{StockMovementFilter, StockMovementRow};
#[allow(unused_imports)]
pub use reservation::{StockReservationFilter, StockReservationRow};

/// `stock_adjustment_line` 集合名（单一来源：`InventoryExt` 关联常量）。
const STOCK_ADJUSTMENT_LINES: &str = <mongodb::Database as InventoryExt>::STOCK_ADJUSTMENT_LINES;
/// `stock_balance` 集合名。
const STOCK_BALANCES: &str = <mongodb::Database as InventoryExt>::STOCK_BALANCES;
/// `stock_reservation` 集合名。
const STOCK_RESERVATIONS: &str = <mongodb::Database as InventoryExt>::STOCK_RESERVATIONS;
/// `stock_movement` 集合名。
const STOCK_MOVEMENTS: &str = <mongodb::Database as InventoryExt>::STOCK_MOVEMENTS;
/// `stock_adjustment` 集合名。
const STOCK_ADJUSTMENTS: &str = <mongodb::Database as InventoryExt>::STOCK_ADJUSTMENTS;
/// `warehouse` 集合名。
const WAREHOUSES: &str = <mongodb::Database as WarehouseExt>::WAREHOUSES;
/// `warehouse_revision` 集合名。
const WAREHOUSE_REVISIONS: &str = <mongodb::Database as WarehouseExt>::WAREHOUSE_REVISIONS;
/// `sku` 集合名。
const SKUS: &str = <mongodb::Database as CatalogExt>::SKUS;
/// `sku_revision` 集合名。
const SKU_REVISIONS: &str = <mongodb::Database as CatalogExt>::SKU_REVISIONS;
/// `purchase_receipt` 集合名。
const PURCHASE_RECEIPTS: &str = <mongodb::Database as FulfillmentExt>::PURCHASE_RECEIPTS;
/// `business_document` 集合名。
const BUSINESS_DOCUMENTS: &str = <mongodb::Database as DocumentRegistryExt>::BUSINESS_DOCUMENTS;

/// D17 域专用仓储：语义查询与多步骤事务写入。
///
/// 本类型屏蔽库存域及页面投影所需跨域引用的 MongoDB 查询细节，并提供必须由
/// Service 传入事务执行器的聚合写入入口，由 `InventoryExt::inventory()` 访问。
pub struct InventoryRepository<'a> {
    db: &'a Database,
}

impl<'a> InventoryRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}
