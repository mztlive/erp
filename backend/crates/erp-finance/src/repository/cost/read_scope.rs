//! 成本范围投影所需的有界候选事实；应用层在分页前裁剪全部分配。
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Result, mongo_ops};
use serde::Deserialize;

use super::{CostEntryFilter, CostEntryRow, cost_entry_projection};
use crate::entity::cost::{CostAllocation, CostEntry};

/// 候选成本上限，超限由调用方整体拒绝，禁止截断合计。
pub const ENTRY_LIMIT: usize = 10_000;
/// 候选分配上限。
pub const ALLOCATION_LIMIT: usize = 100_000;

/// 成本范围版本事实；重验不读取金额及完整来源引用。
#[derive(Debug, Clone, Deserialize)]
pub struct CostEntryScopeVersion {
    /// 成本身份。
    pub id: String,
    /// 成本业务版本。
    pub version: u64,
}

/// 分配版本及授权所需的真实销售来源，不包含金额或展示字段。
#[derive(Debug, Clone, Deserialize)]
pub struct CostAllocationScopeVersion {
    /// 分配身份。
    pub id: String,
    /// 分配业务版本。
    pub version: u64,
    /// 分配对应的成本身份。
    pub cost_entry_id: String,
    /// 真实销售来源，缺失来源保留 None。
    pub sales_order_id: Option<String>,
}

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

    /// 读取完整候选身份和版本；保留新增、删除及非当前页变化检测。
    ///
    /// # 参数
    /// * `filter` - 已规范化的成本筛选。
    /// * `id` - 独立成本筛选。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 最多候选上限加一条，按身份稳定排序。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_candidate_versions(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryScopeVersion>>;

    /// 只装载匹配分配引用的成本投影，空身份集合直接返回空集。
    ///
    /// # 参数
    /// * `filter` - 已规范化的成本筛选。
    /// * `ids` - 当前完整候选集合内的成本身份。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 返回匹配成本，按身份稳定排序。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_entries_by_ids(
        &self,
        filter: &CostEntryFilter,
        ids: &[String],
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

    async fn scope_candidate_versions(
        &self,
        filter: &CostEntryFilter,
        id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryScopeVersion>> {
        let mut document = filter.to_doc();
        if let Some(id) = id {
            document.insert("id", id);
        }
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CostEntryScopeVersion>(),
            document,
            FindOptions::builder()
                .limit((ENTRY_LIMIT + 1) as i64)
                .sort(doc! { "id": 1 })
                .projection(doc! { "id": 1, "version": 1 })
                .build(),
            executor,
        )
        .await
    }

    async fn scope_entries_by_ids(
        &self,
        filter: &CostEntryFilter,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostEntryRow>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let mut document = filter.to_doc();
        document.insert("id", doc! { "$in": ids });
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

    /// 读取成本对应的完整分配版本及销售来源，调用方累计检查上限。
    ///
    /// # 参数
    /// * `ids` - 成本身份集合。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 最多分配上限加一条；空成本集合返回空集。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_allocation_versions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>>;

    /// 联合应用销售单与成本身份筛选，再读取匹配分配的成本引用。
    ///
    /// # 参数
    /// * `cost_entry_id` - 可选成本身份。
    /// * `sales_order_id` - 可选销售单身份。
    /// * `entry_ids` - 同事务中的完整候选成本身份，排除孤立分配。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 最多分配上限加一条，未分页的完整匹配集合。
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_matching_allocations(
        &self,
        cost_entry_id: Option<&str>,
        sales_order_id: Option<&str>,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>>;
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

    async fn scope_allocation_versions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CostAllocationScopeVersion>(),
            doc! { "deleted_at": 0_i64, "cost_entry_id": { "$in": ids } },
            allocation_version_options(),
            executor,
        )
        .await
    }

    async fn scope_matching_allocations(
        &self,
        cost_entry_id: Option<&str>,
        sales_order_id: Option<&str>,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CostAllocationScopeVersion>> {
        if entry_ids.is_empty() {
            return Ok(vec![]);
        }
        let mut filter = matching_allocation_filter(cost_entry_id, sales_order_id);
        filter.insert("$and", vec![doc! { "cost_entry_id": { "$in": entry_ids } }]);
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CostAllocationScopeVersion>(),
            filter,
            allocation_version_options(),
            executor,
        )
        .await
    }
}

/// 业务筛选使用交集，软删除条件始终有效。
fn matching_allocation_filter(cost_entry_id: Option<&str>, sales_order_id: Option<&str>) -> Document {
    let mut filter = doc! { "deleted_at": 0_i64 };
    if let Some(id) = cost_entry_id {
        filter.insert("cost_entry_id", id);
    }
    if let Some(id) = sales_order_id {
        filter.insert("sales_order_id", id);
    }
    filter
}

/// 重验只读取身份、版本、成本引用与真实销售来源。
fn allocation_version_options() -> FindOptions {
    FindOptions::builder()
        .limit((ALLOCATION_LIMIT + 1) as i64)
        .sort(doc! { "id": 1 })
        .projection(doc! { "id": 1, "version": 1, "cost_entry_id": 1, "sales_order_id": 1 })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_allocations_intersect_sales_and_cost_filters() {
        assert_eq!(
            matching_allocation_filter(Some("cost-a"), Some("sales-a")),
            doc! { "deleted_at": 0_i64, "cost_entry_id": "cost-a", "sales_order_id": "sales-a" }
        );
        assert_eq!(
            matching_allocation_filter(None, Some("sales-a")),
            doc! { "deleted_at": 0_i64, "sales_order_id": "sales-a" }
        );
        assert_eq!(matching_allocation_filter(None, None), doc! { "deleted_at": 0_i64 });
    }
}
