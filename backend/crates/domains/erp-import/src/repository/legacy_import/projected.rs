//! 旧数据导入集合共用的列表投影查询。

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::dto::{
    LEGACY_IMPORT_BATCH_SORT_FIELDS, LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS, LEGACY_IMPORT_ROW_SORT_FIELDS,
};

// 用同一执行器和筛选条件完成批次、行、确认列表的投影分页与计数；查询、游标或计数任一步失败即返回，不交付不完整分页。
pub(super) async fn search_projected_page<Entity, Row>(
    base: &mongodb::Collection<Entity>,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: i64,
    projection: Document,
    executor: &mut dyn Executor,
) -> Result<PageResult<Row>>
where
    Entity: Send + Sync,
    Row: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    let options = FindOptions::builder().sort(sort).skip(skip).limit(limit).projection(projection).build();
    let collection = base.clone_with_type::<Row>();
    let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;

    let total = mongo_ops::count_documents(base, filter, executor).await?;
    Ok(PageResult { items, total: total as i64 })
}

// 按白名单构造 MongoDB 排序，未指定时用 `created_at`。DTO 会先校验；直接调用若传非法字段，调试构建触发 `debug_assert`，发布构建回退 `created_at`。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    debug_assert!(sort_by.is_none_or(is_allowed_sort_field), "非法排序字段已回退 created_at：{sort_by:?}");
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|field| is_allowed_sort_field(field)).unwrap_or("created_at");
    doc! { field: direction }
}

// 核对字段是否在批次、行或确认的排序白名单中，避免仓储接收任意字段。
fn is_allowed_sort_field(field: &str) -> bool {
    LEGACY_IMPORT_BATCH_SORT_FIELDS.contains(&field)
        || LEGACY_IMPORT_ROW_SORT_FIELDS.contains(&field)
        || LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS.contains(&field)
}

// 仅选取批次列表展示所需字段，避免加载内部资产关联及整份批次文档。
pub(super) fn legacy_import_batch_projection() -> Document {
    doc! {
        "id": 1,
        "batch_no": 1,
        "source_system_id": 1,
        "source_object_set": 1,
        "baseline_date": 1,
        "import_rule_version": 1,
        "status": 1,
        "total_rows": 1,
        "success_rows": 1,
        "failed_rows": 1,
        "failure_code_summary": 1,
        "confirmation_status_summary": 1,
        "version": 1,
        "created_at": 1,
    }
}

// 限制导入行列表的展示字段，不返回规范化载荷。
pub(super) fn legacy_import_row_projection() -> Document {
    doc! {
        "id": 1,
        "batch_id": 1,
        "source_object_type": 1,
        "source_row_key": 1,
        "parse_status": 1,
        "mapping_status": 1,
        "import_status": 1,
        "external_identity_map_id": 1,
        "error_code": 1,
        "target_document_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

// 只投影确认列表所需的任务、范围及决定字段，避免取回整条确认文档。
pub(super) fn legacy_import_confirmation_projection() -> Document {
    doc! {
        "id": 1,
        "batch_id": 1,
        "confirmation_scope": 1,
        "owner_role": 1,
        "batch_version": 1,
        "trial_version": 1,
        "status": 1,
        "decision": 1,
        "reason_code": 1,
        "work_item_id": 1,
        "decided_by": 1,
        "decided_at": 1,
        "version": 1,
        "created_at": 1,
    }
}
