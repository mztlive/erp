//! `bulk_selection_snapshots` 列表筛选、投影行与仓储查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result};
use serde::{Deserialize, Serialize};

use super::super::page::search_projected_page;
use super::sort_doc;
use crate::entity::bulk_job::{BulkSelectionSnapshot, SelectionStatus, SelectionType};

/// 选择快照列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkSelectionSnapshotRow {
    /// 实体主键。
    pub id: String,
    /// 选择类型。
    pub selection_type: SelectionType,
    /// 数据截止水位（秒级时间戳）。
    pub data_cutoff_at: u64,
    /// 冻结目标数。
    pub item_count: u32,
    /// 创建人。
    pub created_by: String,
    /// 有效期截止时间（秒级时间戳）。
    pub expires_at: u64,
    /// 快照状态。
    pub status: SelectionStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 选择快照列表筛选条件。
#[derive(Debug, Clone)]
pub struct BulkSelectionSnapshotFilter {
    /// 选择类型；`None` 表示不筛选。
    pub selection_type: Option<SelectionType>,
    /// 快照状态；`None` 表示不筛选。
    pub status: Option<SelectionStatus>,
    /// 创建人；`None` 表示不筛选。
    pub created_by: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for BulkSelectionSnapshotFilter {
    /// 缺省分页从第一页、每页二十条开始，其余筛选保持空条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回第 1 页、每页 20 条的空筛选条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            selection_type: None,
            status: None,
            created_by: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for BulkSelectionSnapshotFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(selection_type) = self.selection_type {
            filter.insert("selection_type", selection_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(created_by) = &self.created_by {
            filter.insert("created_by", created_by);
        }
        filter
    }
}

impl Pagination for BulkSelectionSnapshotFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 选择快照集合仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait BulkSelectionSnapshotRepositoryExt {
    /// 分页检索选择快照列表（投影查询）。
    ///
    /// 只返回 [`BulkSelectionSnapshotRow`] 所需的列表字段，不加载整文档；
    /// `created_by` 精确匹配覆盖 `idx_bulk_selection_snapshots_created`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_snapshots(
        &self,
        filter: &BulkSelectionSnapshotFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BulkSelectionSnapshotRow>>;
}

impl BulkSelectionSnapshotRepositoryExt for persistence_core::Repository<'_, BulkSelectionSnapshot> {
    async fn search_snapshots(
        &self,
        filter: &BulkSelectionSnapshotFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BulkSelectionSnapshotRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(snapshot_projection())
            .build();
        let collection = self.collection().clone_with_type::<BulkSelectionSnapshotRow>();
        search_projected_page(&self.collection(), &collection, filter, options, executor).await
    }
}

/// 选择快照列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn snapshot_projection() -> Document {
    doc! {
        "id": 1,
        "selection_type": 1,
        "data_cutoff_at": 1,
        "item_count": 1,
        "created_by": 1,
        "expires_at": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::BulkSelectionSnapshotFilter;
    use crate::entity::bulk_job::{SelectionStatus, SelectionType};

    #[test]
    fn snapshot_filter_applies_type_status_and_creator() {
        let filter = BulkSelectionSnapshotFilter {
            selection_type: Some(SelectionType::Export),
            status: Some(SelectionStatus::Confirmed),
            created_by: Some("admin-1".to_string()),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("selection_type").unwrap(), "export");
        assert_eq!(document.get_str("status").unwrap(), "confirmed");
        assert_eq!(document.get_str("created_by").unwrap(), "admin-1");
    }
}
