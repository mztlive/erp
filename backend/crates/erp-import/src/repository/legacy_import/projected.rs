//! Shared projected list queries for legacy import collections.

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::dto::{
    LEGACY_IMPORT_BATCH_SORT_FIELDS, LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS, LEGACY_IMPORT_ROW_SORT_FIELDS,
};

/// 执行通用筛选分页投影查询（三类列表共用；查询语义与返回形状不变）。
///
/// # 参数
/// * `base` - 基集合句柄（用于计数）
/// * `filter` - 查询条件文档
/// * `sort` - 排序文档
/// * `skip` - 跳过行数
/// * `limit` - 单页条数
/// * `projection` - 投影文档
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回当前页投影行与满足筛选条件的总数。
///
/// # 错误
/// 当 MongoDB 查询、游标读取或计数失败时返回错误。
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

/// 构建排序文档（排序字段白名单映射，非法字段回退 `created_at`）。
///
/// 非法字段回退保留：DTO 层已做白名单校验，仓储层回退只为直接调用提供
/// 确定性排序；`debug_assert` 在测试中暴露误传。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    debug_assert!(sort_by.is_none_or(is_allowed_sort_field), "非法排序字段已回退 created_at：{sort_by:?}");
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|field| is_allowed_sort_field(field)).unwrap_or("created_at");
    doc! { field: direction }
}

/// 三类列表 DTO 排序白名单的并集；仓储层非法字段仍回退 `created_at`。
fn is_allowed_sort_field(field: &str) -> bool {
    LEGACY_IMPORT_BATCH_SORT_FIELDS.contains(&field)
        || LEGACY_IMPORT_ROW_SORT_FIELDS.contains(&field)
        || LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS.contains(&field)
}

/// 导入批次列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
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

/// 导入行列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
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

/// 导入确认列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
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
