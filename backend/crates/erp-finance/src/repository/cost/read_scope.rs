//! 成本范围投影所需的有界候选事实；应用层在分页前裁剪全部分配。
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Result, mongo_ops};

use super::{CostEntryFilter, CostEntryRow, cost_entry_projection};
use crate::entity::cost::{CostAllocation, CostEntry};

/// 候选成本上限，超限由调用方整体拒绝，禁止截断合计。
pub const ENTRY_LIMIT: usize = 10_000;
/// 候选分配上限。
pub const ALLOCATION_LIMIT: usize = 100_000;

#[allow(async_fn_in_trait)]
pub trait CostEntryReadScopeExt {
    /// 装载业务筛选下完整的有界候选成本，不在权限裁剪前分页。
    ///
    /// # 返回
    /// 最多上限加一条；精确 ID 只用于独立详情。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_candidates(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryRow>>;
}

impl CostEntryReadScopeExt for persistence_core::Repository<'_, CostEntry> {
    async fn scope_candidates(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryRow>> {
        let mut document = filter.to_doc();
        if let Some(id) = id {
            document.insert("id", id);
        }
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CostEntryRow>(),
            document,
            FindOptions::builder()
                .limit((ENTRY_LIMIT + 1) as i64)
                .sort(doc! { "id": 1 })
                .projection(cost_entry_projection())
                .build(),
            executor,
        )
        .await
    }
}

#[allow(async_fn_in_trait)]
pub trait CostAllocationReadScopeExt {
    /// 批量读取成本对应分配，调用方必须检查上限并跨批次累计。
    ///
    /// # 返回
    /// 返回最多分配上限加一条；空成本集合返回空集。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_allocations(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>>;
}

impl CostAllocationReadScopeExt for persistence_core::Repository<'_, CostAllocation> {
    async fn scope_allocations(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocation>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        mongo_ops::find_many(
            &self.collection(),
            doc! { "deleted_at": 0_i64, "cost_entry_id": { "$in": ids } },
            FindOptions::builder().limit((ALLOCATION_LIMIT + 1) as i64).sort(doc! { "id": 1 }).build(),
            executor,
        )
        .await
    }
}
