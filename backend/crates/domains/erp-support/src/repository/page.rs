//! 支撑领域列表共用分页筛选和排序约定。

use mongodb::Collection;
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, QueryFilter, Result, mongo_ops};
use serde::de::DeserializeOwned;

// 在同一集合上用相同筛选读取投影页及总数。
pub(super) async fn search_projected_page<T, Row>(
    collection: &Collection<T>,
    filter: &(impl QueryFilter + Sync),
    options: FindOptions,
    executor: &mut dyn Executor,
) -> Result<PageResult<Row>>
where
    T: Send + Sync,
    Row: DeserializeOwned + Send + Sync,
{
    let document = filter.to_doc();
    let rows = collection.clone_with_type::<Row>();
    let items = mongo_ops::find_many(&rows, document.clone(), options, executor).await?;
    let total = mongo_ops::count_documents(collection, document, executor).await?;
    Ok(PageResult { items, total: total as i64 })
}

// 返回 MongoDB 排序方向（升序 `1`，降序 `-1`）。
// 三个列表 `sort_doc` 共用同一方向口径；字段白名单仍由各自保留（来源注册透传 Service 已校验字段，快照/任务/资产限制 `created_at`/`updated_at`）。
// 返回 MongoDB 排序方向数值。
// `ascending` 为是否升序；`false` 表示降序（默认）。
pub(crate) fn sort_direction(ascending: bool) -> i32 {
    if ascending { 1 } else { -1 }
}

// 返回 `created_at`/`updated_at` 白名单内的排序字段。
// 未知字段回落默认 `created_at`，与既有快照/任务/资产行为一致；来源注册列表透传 Service 已校验字段，不使用本函数。
// 返回白名单内的排序字段名。
// `sort_by` 为排序字段；`None` 或白名单外字段时默认 `created_at`。
pub(crate) fn created_updated_field(sort_by: Option<&str>) -> &'static str {
    match sort_by {
        Some("updated_at") => "updated_at",
        _ => "created_at",
    }
}

#[cfg(test)]
mod tests {
    use super::{created_updated_field, sort_direction};

    #[test]
    fn sort_helpers_keep_direction_and_whitelist() {
        assert_eq!(sort_direction(true), 1);
        assert_eq!(sort_direction(false), -1);
        assert_eq!(created_updated_field(None), "created_at");
        assert_eq!(created_updated_field(Some("updated_at")), "updated_at");
        assert_eq!(created_updated_field(Some("job_no")), "created_at");
    }
}
