//! 域 D23 `mall_sync` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as MallSyncExt>::MALL_SALES_SYNC_JOBS` 等值。

use entities::mall_sync::{
    MallSalesOrderSnapshot, MallSalesReconciliationItem, MallSalesReconciliationJob, MallSalesSyncCursor,
    MallSalesSyncJob, MasterMappingTask,
};
use mongodb::Database;

use super::super::mall_sync::{
    MallSalesOrderSnapshotFilter, MallSalesReconciliationItemFilter, MallSalesReconciliationJobFilter,
    MallSalesSyncJobFilter, MallSyncRepository, MasterMappingTaskFilter,
};
use crate::Repository;

/// 域 D23 仓储访问器。
pub trait MallSyncExt {
    /// `mall_sales_sync_job` 集合名。
    const MALL_SALES_SYNC_JOBS: &'static str = "mall_sales_sync_jobs";
    /// `mall_sales_sync_cursor` 集合名。
    const MALL_SALES_SYNC_CURSORS: &'static str = "mall_sales_sync_cursors";
    /// `mall_sales_order_snapshot` 集合名。
    const MALL_SALES_ORDER_SNAPSHOTS: &'static str = "mall_sales_order_snapshots";
    /// `mall_sales_reconciliation_job` 集合名。
    const MALL_SALES_RECONCILIATION_JOBS: &'static str = "mall_sales_reconciliation_jobs";
    /// `mall_sales_reconciliation_item` 集合名。
    const MALL_SALES_RECONCILIATION_ITEMS: &'static str = "mall_sales_reconciliation_items";
    /// `master_mapping_task` 集合名。
    const MASTER_MAPPING_TASKS: &'static str = "master_mapping_tasks";

    /// 同步作业列表筛选条件类型（定义见 `repository::mall_sync`）。
    type MallSalesSyncJobFilter;

    /// 快照列表筛选条件类型（定义见 `repository::mall_sync`）。
    type MallSalesOrderSnapshotFilter;

    /// 核对作业列表筛选条件类型（定义见 `repository::mall_sync`）。
    type MallSalesReconciliationJobFilter;

    /// 核对差异明细列表筛选条件类型（定义见 `repository::mall_sync`）。
    type MallSalesReconciliationItemFilter;

    /// 映射任务列表筛选条件类型（定义见 `repository::mall_sync`）。
    type MasterMappingTaskFilter;

    /// 获取 `mall_sales_sync_job` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_sync::MallSalesSyncJob>`。
    fn mall_sales_sync_jobs(&self) -> Repository<'_, MallSalesSyncJob>;

    /// 获取 `mall_sales_sync_cursor` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_sync::MallSalesSyncCursor>`。
    fn mall_sales_sync_cursors(&self) -> Repository<'_, MallSalesSyncCursor>;

    /// 获取 `mall_sales_order_snapshot` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_sync::MallSalesOrderSnapshot>`。
    fn mall_sales_order_snapshots(&self) -> Repository<'_, MallSalesOrderSnapshot>;

    /// 获取 `mall_sales_reconciliation_job` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_sync::MallSalesReconciliationJob>`。
    fn mall_sales_reconciliation_jobs(&self) -> Repository<'_, MallSalesReconciliationJob>;

    /// 获取 `mall_sales_reconciliation_item` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_sync::MallSalesReconciliationItem>`。
    fn mall_sales_reconciliation_items(&self) -> Repository<'_, MallSalesReconciliationItem>;

    /// 获取 `master_mapping_task` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_sync::MasterMappingTask>`。
    fn master_mapping_tasks(&self) -> Repository<'_, MasterMappingTask>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `MallSyncRepository` 实例。
    fn mall_sync(&self) -> MallSyncRepository<'_>;
}

impl MallSyncExt for Database {
    type MallSalesSyncJobFilter = MallSalesSyncJobFilter;
    type MallSalesOrderSnapshotFilter = MallSalesOrderSnapshotFilter;
    type MallSalesReconciliationJobFilter = MallSalesReconciliationJobFilter;
    type MallSalesReconciliationItemFilter = MallSalesReconciliationItemFilter;
    type MasterMappingTaskFilter = MasterMappingTaskFilter;

    fn mall_sales_sync_jobs(&self) -> Repository<'_, MallSalesSyncJob> {
        Repository::new(self, Self::MALL_SALES_SYNC_JOBS)
    }

    fn mall_sales_sync_cursors(&self) -> Repository<'_, MallSalesSyncCursor> {
        Repository::new(self, Self::MALL_SALES_SYNC_CURSORS)
    }

    fn mall_sales_order_snapshots(&self) -> Repository<'_, MallSalesOrderSnapshot> {
        Repository::new(self, Self::MALL_SALES_ORDER_SNAPSHOTS)
    }

    fn mall_sales_reconciliation_jobs(&self) -> Repository<'_, MallSalesReconciliationJob> {
        Repository::new(self, Self::MALL_SALES_RECONCILIATION_JOBS)
    }

    fn mall_sales_reconciliation_items(&self) -> Repository<'_, MallSalesReconciliationItem> {
        Repository::new(self, Self::MALL_SALES_RECONCILIATION_ITEMS)
    }

    fn master_mapping_tasks(&self) -> Repository<'_, MasterMappingTask> {
        Repository::new(self, Self::MASTER_MAPPING_TASKS)
    }

    fn mall_sync(&self) -> MallSyncRepository<'_> {
        MallSyncRepository::new(self)
    }
}
