//! 域 D20 `cost` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as CostExt>::COST_ENTRIES` 等值。

use entities::cost::{CostAllocation, CostEntry};
use mongodb::Database;

use super::super::cost::{CostAllocationFilter, CostEntryFilter, CostEntryRow, CostRepository};
use crate::Repository;

/// 域 D20 仓储访问器。
pub trait CostExt {
    /// `cost_entry` 集合名。
    const COST_ENTRIES: &'static str = "cost_entries";
    /// `cost_allocation` 集合名。
    const COST_ALLOCATIONS: &'static str = "cost_allocations";

    /// 成本事实列表筛选条件类型（定义见 `repository::cost`）。
    type CostEntryFilter;

    /// 成本事实列表持久化投影类型（定义见 `repository::cost`）。
    type CostEntryRow;

    /// 成本分配列表筛选条件类型（定义见 `repository::cost`）。
    type CostAllocationFilter;

    /// 获取 `cost_entry` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::cost::CostEntry>`。
    fn cost_entries(&self) -> Repository<'_, CostEntry>;

    /// 获取 `cost_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::cost::CostAllocation>`。
    fn cost_allocations(&self) -> Repository<'_, CostAllocation>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `CostRepository` 实例。
    fn cost(&self) -> CostRepository<'_>;
}

impl CostExt for Database {
    type CostEntryFilter = CostEntryFilter;
    type CostEntryRow = CostEntryRow;
    type CostAllocationFilter = CostAllocationFilter;

    fn cost_entries(&self) -> Repository<'_, CostEntry> {
        Repository::new(self, Self::COST_ENTRIES)
    }

    fn cost_allocations(&self) -> Repository<'_, CostAllocation> {
        Repository::new(self, Self::COST_ALLOCATIONS)
    }

    fn cost(&self) -> CostRepository<'_> {
        CostRepository::new(self)
    }
}
