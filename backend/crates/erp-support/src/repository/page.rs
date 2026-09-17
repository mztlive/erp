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
