//! `bulk_selection_items` 列表投影行与仓储查询。

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::entity::bulk_job::{BulkSelectionItem, BulkSelectionSnapshotId, SelectionItemStatus};

/// 选择项逐项结果投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkSelectionItemRow {
    /// 实体主键。
    pub id: String,
    /// 所属选择快照 ID（与查询过滤一致，投影已覆盖）。
    pub selection_snapshot_id: String,
    /// 目标对象类型代码。
    pub object_type: String,
    /// 目标对象 ID。
    pub object_id: String,
    /// 预览时版本。
    pub expected_version: Option<String>,
    /// 预览时内容摘要。
    pub expected_hash: Option<String>,
    /// 逐项执行结果（未执行为 `None`）。
    pub result_status: Option<SelectionItemStatus>,
    /// 失败原因代码（适用时）。
    pub result_code: Option<String>,
}

/// 选择项集合仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait BulkSelectionItemRepositoryExt {
    /// 分页检索快照逐项结果（投影查询）。
    /// 只返回 [`BulkSelectionItemRow`] 所需的逐项字段，不加载整文档；`result_status` 过滤覆盖 `idx_bulk_selection_items_result`。
    /// 返回当前页逐项结果行与总数。
    /// `snapshot_id` 为选择快照 ID；`result_status` 为逐项执行结果；`None` 表示不筛选。
    /// 沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询、游标读取或计数错误。
    async fn search_items(
        &self,
        snapshot_id: &BulkSelectionSnapshotId,
        result_status: Option<SelectionItemStatus>,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BulkSelectionItemRow>>;
}

impl BulkSelectionItemRepositoryExt for persistence_core::Repository<'_, BulkSelectionItem> {
    async fn search_items(
        &self,
        snapshot_id: &BulkSelectionSnapshotId,
        result_status: Option<SelectionItemStatus>,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BulkSelectionItemRow>> {
        let mut filter = doc! { "selection_snapshot_id": snapshot_id.to_string() };
        if let Some(result_status) = result_status {
            filter.insert("result_status", result_status.as_str());
        }

        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1 })
            .skip((page.max(1) - 1) * u64::from(page_size))
            .limit(i64::from(page_size))
            .projection(selection_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<BulkSelectionItemRow>();

        let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter, executor).await?;
        Ok(PageResult { items, total: total as i64 })
    }
}

// 选择项逐项结果投影字段。
fn selection_item_projection() -> Document {
    doc! {
        "id": 1,
        "selection_snapshot_id": 1,
        "object_type": 1,
        "object_id": 1,
        "expected_version": 1,
        "expected_hash": 1,
        "result_status": 1,
        "result_code": 1,
    }
}
