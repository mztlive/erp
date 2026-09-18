//! 分页投影查询的共享骨架：一次构造查询文档，复用给 `find` 与 `count`。
//!
//! 五个列表搜索（快照/后台任务/文件资产/来源系统/外部身份映射）原先各自对
//! 同一 `filter` 调用两次 `to_doc()`，每次重建查询文档；本 helper 单次构造
//! 后复用，查询条件与总数语义不变。

use mongodb::Collection;
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, mongo_ops};
use serde::de::DeserializeOwned;

/// 执行投影分页查询：单次构造 `filter` 文档并复用于列表与计数。
///
/// # 参数
/// * `collection` - 实体集合（用于 `count`）
/// * `rows` - 同集合的投影行类型集合（用于 `find`）
/// * `filter` - 筛选与分页条件（`QueryFilter + Pagination`）
/// * `options` - 已组装的排序/跳过/条数/投影选项
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回当前页投影行与满足筛选条件的总数。
///
/// # 错误
/// 当 MongoDB 查询、游标读取或计数失败时返回错误。
pub(crate) async fn search_projected_page<T, Row, Filter>(
    collection: &Collection<T>,
    rows: &Collection<Row>,
    filter: &Filter,
    options: FindOptions,
    executor: &mut dyn Executor,
) -> Result<PageResult<Row>>
where
    T: Send + Sync,
    Row: DeserializeOwned + Send + Sync,
    Filter: QueryFilter + Pagination + Send + Sync,
{
    let document = filter.to_doc();
    let items = mongo_ops::find_many(rows, document.clone(), options, executor).await?;
    let total = mongo_ops::count_documents(collection, document, executor).await?;
    Ok(PageResult { items, total: total as i64 })
}

/// 返回 MongoDB 排序方向（升序 `1`，降序 `-1`）。
///
/// 三个列表 `sort_doc` 共用同一方向口径；字段白名单仍由各自保留
/// （来源注册透传 Service 已校验字段，快照/任务/资产限制
/// `created_at`/`updated_at`）。
///
/// # 参数
/// * `ascending` - 是否升序；`false` 表示降序（默认）
///
/// # 返回
/// 返回 MongoDB 排序方向数值。
pub(crate) fn sort_direction(ascending: bool) -> i32 {
    if ascending { 1 } else { -1 }
}

/// 返回 `created_at`/`updated_at` 白名单内的排序字段。
///
/// 未知字段回落默认 `created_at`，与既有快照/任务/资产行为一致；
/// 来源注册列表透传 Service 已校验字段，不使用本函数。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或白名单外字段时默认 `created_at`
///
/// # 返回
/// 返回白名单内的排序字段名。
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
