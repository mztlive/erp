//! 域 D31 `mall_backfill` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 两侧统一取
//! `<mongodb::Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_JOBS` 等值，
//! 禁止字面量重复。
//!
//! 回填明细是不可变执行结果（§4.5），只暴露只读追加仓储，不暴露带软删除
//! 方法的通用 `Repository`；回填作业走通用 `Repository`（状态推进与进度统计
//! 使用乐观锁 CAS）。

use entities::mall_backfill::MallConsumptionBackfillJob;
use mongodb::Database;

use super::super::mall_backfill::{
    MallBackfillRepository, MallConsumptionBackfillItemFilter, MallConsumptionBackfillItemRepository,
    MallConsumptionBackfillJobFilter,
};
use crate::Repository;

/// 域 D31 仓储访问器。
pub trait MallBackfillExt {
    /// `mall_consumption_backfill_job` 集合名。
    const MALL_CONSUMPTION_BACKFILL_JOBS: &'static str = "mall_consumption_backfill_jobs";
    /// `mall_consumption_backfill_item` 集合名。
    const MALL_CONSUMPTION_BACKFILL_ITEMS: &'static str = "mall_consumption_backfill_items";

    /// 回填作业列表筛选条件类型（定义见 `repository::mall_backfill`）。
    type MallConsumptionBackfillJobFilter;

    /// 回填明细列表筛选条件类型（定义见 `repository::mall_backfill`）。
    type MallConsumptionBackfillItemFilter;

    /// 获取 `mall_consumption_backfill_job` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_backfill::MallConsumptionBackfillJob>`。
    fn mall_consumption_backfill_jobs(&self) -> Repository<'_, MallConsumptionBackfillJob>;

    /// 获取 `mall_consumption_backfill_item` 集合的只读追加仓储。
    ///
    /// 回填明细是不可变执行结果（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallConsumptionBackfillItemRepository` 实例。
    fn mall_consumption_backfill_items(&self) -> MallConsumptionBackfillItemRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `MallBackfillRepository` 实例。
    fn mall_backfill(&self) -> MallBackfillRepository<'_>;
}

impl MallBackfillExt for Database {
    type MallConsumptionBackfillJobFilter = MallConsumptionBackfillJobFilter;
    type MallConsumptionBackfillItemFilter = MallConsumptionBackfillItemFilter;

    fn mall_consumption_backfill_jobs(&self) -> Repository<'_, MallConsumptionBackfillJob> {
        Repository::new(self, Self::MALL_CONSUMPTION_BACKFILL_JOBS)
    }

    fn mall_consumption_backfill_items(&self) -> MallConsumptionBackfillItemRepository<'_> {
        MallConsumptionBackfillItemRepository::new(self)
    }

    fn mall_backfill(&self) -> MallBackfillRepository<'_> {
        MallBackfillRepository::new(self)
    }
}
