//! 域 D04 `bulk_job` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as BulkJobExt>::BULK_SELECTION_SNAPSHOTS` 等值。

use crate::repository::owned::{
    BackgroundJobItemRepository, BackgroundJobRepository, BulkSelectionItemRepository,
    BulkSelectionSnapshotRepository,
};
use mongodb::Database;

use super::super::bulk_job::{BackgroundJobFilter, BulkJobRepository, BulkSelectionSnapshotFilter};

/// 域 D04 仓储访问器。
pub trait BulkJobExt {
    /// `bulk_selection_snapshot` 集合名。
    const BULK_SELECTION_SNAPSHOTS: &'static str = "bulk_selection_snapshots";
    /// `bulk_selection_item` 集合名。
    const BULK_SELECTION_ITEMS: &'static str = "bulk_selection_items";
    /// `background_job` 集合名。
    const BACKGROUND_JOBS: &'static str = "background_jobs";
    /// `background_job_item` 集合名。
    const BACKGROUND_JOB_ITEMS: &'static str = "background_job_items";

    /// 选择快照列表筛选条件类型（定义见 `repository::bulk_job`）。
    type BulkSelectionSnapshotFilter;

    /// 后台任务列表筛选条件类型（定义见 `repository::bulk_job`）。
    type BackgroundJobFilter;

    /// 获取 `bulk_selection_snapshot` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `BulkSelectionSnapshotRepository<'_>`。
    fn bulk_selection_snapshots(&self) -> BulkSelectionSnapshotRepository<'_>;

    /// 获取 `bulk_selection_item` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `BulkSelectionItemRepository<'_>`。
    fn bulk_selection_items(&self) -> BulkSelectionItemRepository<'_>;

    /// 获取 `background_job` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `BackgroundJobRepository<'_>`。
    fn background_jobs(&self) -> BackgroundJobRepository<'_>;

    /// 获取 `background_job_item` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `BackgroundJobItemRepository<'_>`。
    fn background_job_items(&self) -> BackgroundJobItemRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `BulkJobRepository` 实例。
    fn bulk_job(&self) -> BulkJobRepository<'_>;
}

impl BulkJobExt for Database {
    type BulkSelectionSnapshotFilter = BulkSelectionSnapshotFilter;
    type BackgroundJobFilter = BackgroundJobFilter;

    fn bulk_selection_snapshots(&self) -> BulkSelectionSnapshotRepository<'_> {
        BulkSelectionSnapshotRepository::new(self, Self::BULK_SELECTION_SNAPSHOTS)
    }

    fn bulk_selection_items(&self) -> BulkSelectionItemRepository<'_> {
        BulkSelectionItemRepository::new(self, Self::BULK_SELECTION_ITEMS)
    }

    fn background_jobs(&self) -> BackgroundJobRepository<'_> {
        BackgroundJobRepository::new(self, Self::BACKGROUND_JOBS)
    }

    fn background_job_items(&self) -> BackgroundJobItemRepository<'_> {
        BackgroundJobItemRepository::new(self, Self::BACKGROUND_JOB_ITEMS)
    }

    fn bulk_job(&self) -> BulkJobRepository<'_> {
        BulkJobRepository::new(self)
    }
}
