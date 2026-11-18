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
    // 初始化批量选择快照筛选：第一页、每页 20 条，业务筛选默认留空。
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
    // 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    // `selection_type` 与 `status` 为 `Some` 时分别写入 `as_str()`。`created_by` 为 `Some` 时精确匹配。分页与排序字段不进入条件。
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
    // 返回选择快照筛选保存的页码与单页条数，不把小于 1 的页码归一成第一页。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 选择快照集合仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait BulkSelectionSnapshotRepositoryExt {
    /// 分页检索选择快照列表（投影查询）。
    /// 只返回 [`BulkSelectionSnapshotRow`] 所需的列表字段，不加载整文档；`created_by` 精确匹配覆盖 `idx_bulk_selection_snapshots_created`。
    /// 返回当前页投影行与满足筛选条件的总数。
    /// 沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询、游标读取或计数错误。
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
        search_projected_page(&self.collection(), filter, options, executor).await
    }
}

// 选择快照列表投影字段。
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
