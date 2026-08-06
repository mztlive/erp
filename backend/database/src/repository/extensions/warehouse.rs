//! 域 D11 `warehouse` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as WarehouseExt>::WAREHOUSES` 等值。

use entities::warehouse::{Warehouse, WarehouseRevision, WarehouseSkuPolicy};
use mongodb::Database;

use super::super::warehouse::{
    WarehouseFilter, WarehouseRepository, WarehouseRevisionFilter, WarehouseSkuPolicyFilter,
};
use crate::Repository;

/// 域 D11 仓储访问器。
pub trait WarehouseExt {
    /// `warehouse` 集合名。
    const WAREHOUSES: &'static str = "warehouses";
    /// `warehouse_revision` 集合名。
    const WAREHOUSE_REVISIONS: &'static str = "warehouse_revisions";
    /// `warehouse_sku_policy` 集合名。
    const WAREHOUSE_SKU_POLICIES: &'static str = "warehouse_sku_policies";

    /// 仓库列表筛选条件类型（定义见 `repository::warehouse`）。
    type WarehouseFilter;

    /// 仓库修订列表筛选条件类型（定义见 `repository::warehouse`）。
    type WarehouseRevisionFilter;

    /// 仓库-SKU 预警策略列表筛选条件类型（定义见 `repository::warehouse`）。
    type WarehouseSkuPolicyFilter;

    /// 获取 `warehouse` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::warehouse::Warehouse>`。
    fn warehouses(&self) -> Repository<'_, Warehouse>;

    /// 获取 `warehouse_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::warehouse::WarehouseRevision>`。
    fn warehouse_revisions(&self) -> Repository<'_, WarehouseRevision>;

    /// 获取 `warehouse_sku_policy` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::warehouse::WarehouseSkuPolicy>`。
    fn warehouse_sku_policies(&self) -> Repository<'_, WarehouseSkuPolicy>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `WarehouseRepository` 实例。
    fn warehouse(&self) -> WarehouseRepository<'_>;
}

impl WarehouseExt for Database {
    type WarehouseFilter = WarehouseFilter;
    type WarehouseRevisionFilter = WarehouseRevisionFilter;
    type WarehouseSkuPolicyFilter = WarehouseSkuPolicyFilter;

    /// 获取 `warehouse` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::warehouse::Warehouse>`。
    fn warehouses(&self) -> Repository<'_, Warehouse> {
        Repository::new(self, Self::WAREHOUSES)
    }

    /// 获取 `warehouse_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::warehouse::WarehouseRevision>`。
    fn warehouse_revisions(&self) -> Repository<'_, WarehouseRevision> {
        Repository::new(self, Self::WAREHOUSE_REVISIONS)
    }

    /// 获取 `warehouse_sku_policy` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::warehouse::WarehouseSkuPolicy>`。
    fn warehouse_sku_policies(&self) -> Repository<'_, WarehouseSkuPolicy> {
        Repository::new(self, Self::WAREHOUSE_SKU_POLICIES)
    }

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `WarehouseRepository` 实例。
    fn warehouse(&self) -> WarehouseRepository<'_> {
        WarehouseRepository::new(self)
    }
}
