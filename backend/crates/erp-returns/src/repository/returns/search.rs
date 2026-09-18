//! 投影分页查询、排序白名单与原事实 `$in` 筛选的 crate 内共用实现。

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Repository, Result, mongo_ops};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// 列表筛选的排序字段（三份 Filter 的 `sort_by` / `sort_ascending` 同构读取）。
pub(super) trait ListSort {
    fn sort_by(&self) -> Option<&str>;
    fn sort_ascending(&self) -> bool;
}

/// 执行投影分页查询：组装 `FindOptions`、按行类型投影，并对同一 `filter` 调用两次
/// `to_doc()`（分别用于 `find_many` 与 `count_documents`）。
///
/// # 参数
/// * `repo` - 实体集合仓储
/// * `filter` - 筛选、分页与排序条件
/// * `projection` - 列表投影字段
/// * `allowed_sort` - 排序字段白名单；未命中回退 `created_at`
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回当前页投影行与满足筛选条件的总数。
///
/// # 错误
/// 当 MongoDB 查询、游标读取或计数失败时返回错误。
pub(super) async fn search_projected<Row, Entity, Filter>(
    repo: &Repository<'_, Entity>,
    filter: &Filter,
    projection: Document,
    allowed_sort: &[&str],
    executor: &mut dyn Executor,
) -> Result<PageResult<Row>>
where
    Row: DeserializeOwned + Send + Sync,
    Entity: Serialize + DeserializeOwned + Send + Sync,
    Filter: QueryFilter + Pagination + ListSort,
{
    let options = FindOptions::builder()
        .sort(sort_doc(filter.sort_by(), filter.sort_ascending(), allowed_sort))
        .skip(filter.skip())
        .limit(filter.limit())
        .projection(projection)
        .build();
    let collection = repo.collection().clone_with_type::<Row>();
    let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
    let total = mongo_ops::count_documents(&repo.collection(), filter.to_doc(), executor).await?;
    Ok(PageResult { items, total: total as i64 })
}

/// 构建排序文档：字段名经白名单映射，未命中回退 `created_at` 降序。
///
/// # 参数
/// * `sort_by` - 排序字段（白名单内有效）
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段名集合（防止透传任意字段名）
///
/// # 返回
/// 返回排序条件文档；`id` 作为次键保证同值稳定排序。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|name| allowed.contains(name)).unwrap_or("created_at");
    doc! { field: direction, "id": direction }
}

/// 两类原事实引用的共用 `$in` 筛选（字段名由调用方保持原语义）。
pub(super) fn originals_filter<A: ToString, B: ToString>(
    ids_a: &[A],
    field_a: &str,
    ids_b: &[B],
    field_b: &str,
) -> Document {
    let mut filter = Document::new();
    if !ids_a.is_empty() {
        let ids: Vec<String> = ids_a.iter().map(ToString::to_string).collect();
        filter.insert(field_a, doc! { "$in": ids });
    }
    if !ids_b.is_empty() {
        let ids: Vec<String> = ids_b.iter().map(ToString::to_string).collect();
        filter.insert(field_b, doc! { "$in": ids });
    }
    filter
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::sort_doc;

    #[test]
    fn sort_doc_appends_id_tiebreaker_for_both_directions() {
        assert_eq!(
            sort_doc(Some("occurred_at"), true, &["occurred_at", "amount", "created_at"]),
            doc! { "occurred_at": 1, "id": 1 }
        );
        assert_eq!(
            sort_doc(Some("amount"), false, &["occurred_at", "amount", "created_at"]),
            doc! { "amount": -1, "id": -1 }
        );
    }
}
