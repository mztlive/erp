use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, QueryFilter, Result, mongo_ops};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::repository::CatalogExt;

/// `product_category` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const PRODUCT_CATEGORIES: &str = <mongodb::Database as CatalogExt>::PRODUCT_CATEGORIES;
/// `product_revision` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const PRODUCT_REVISIONS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISIONS;
/// `sku` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const SKUS: &str = <mongodb::Database as CatalogExt>::SKUS;
/// `sku_revision` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const SKU_REVISIONS: &str = <mongodb::Database as CatalogExt>::SKU_REVISIONS;

/// 构造 ID 集合批量匹配条件。
///
/// # 参数
/// * `field` - 匹配字段名
/// * `values` - 待匹配的 ID 字符串集合
///
/// # 返回
/// 返回批量查询条件文档。
pub(super) fn in_filter(field: &str, values: impl IntoIterator<Item = String>) -> Document {
    let values: Vec<Bson> = values.into_iter().map(Bson::String).collect();
    doc! { field: { "$in": values } }
}

/// 空集合早退的批量查询条件（erp-catalog-005）。
///
/// 各仓储 `find_by_ids/find_by_product_ids/find_by_sku_ids`
/// 与 `find_media_by_revision_ids` 均重复“空集合早退 + `in_filter` 批量查”
/// 两行模式；各方法只声明字段名并委托至此，空输入不再访问数据库。
///
/// # 参数
/// * `field` - 匹配字段名
/// * `ids` - 待匹配的 ID 集合；空集合返回 `None`
///
/// # 返回
/// 空集合返回 `None`（调用方直接返回空集合）；否则返回批量查询条件。
pub(super) fn batch_ids_filter<Id: ToString>(field: &str, ids: &[Id]) -> Option<Document> {
    if ids.is_empty() {
        return None;
    }
    Some(in_filter(field, ids.iter().map(ToString::to_string)))
}

/// 构建排序文档。
///
/// # 参数
/// * `field` - 已通过白名单校验的排序字段
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(field: &str, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction, "id": direction }
}

/// 缺省分页四件套（erp-catalog-011）。
///
/// 各 `*Filter::default` 的分页字面量唯一来源：页码 1、单页 20 条、
/// 无排序字段、降序；各实现解构复用，避免多处复制。
///
/// # 返回
/// 返回 `(页码, 单页条数, 排序字段, 是否升序)`。
pub(super) fn default_paging() -> (u64, u32, Option<String>, bool) {
    (1, 20, None, false)
}

/// 白名单排序字段解析（erp-catalog-015）。
///
/// DTO 校验与仓储 `sort_doc` 共用同一白名单表（`dto::catalog` 重导出的
/// `*_SORT_FIELDS` 常量）；未知字段回退默认 `created_at` 的行为保持不变，
/// 新增排序字段时只需改 DTO 侧常量表。
///
/// # 参数
/// * `sort_by` - 待校验的排序字段
/// * `allowed` - 该列表的白名单表
///
/// # 返回
/// 白名单命中返回原字段；否则返回 `created_at`。
pub(super) fn whitelisted_sort<'x>(sort_by: Option<&'x str>, allowed: &[&str]) -> &'x str {
    match sort_by {
        Some(field) if allowed.contains(&field) => field,
        _ => "created_at",
    }
}

/// 最大修订号投影行（erp-catalog-008）。
#[derive(Deserialize)]
pub(super) struct RevisionNoRow {
    revision_no: u32,
}

/// 按修订号降序单文档读取历史最大修订号（erp-catalog-008）。
///
/// `latest_*_revision_no` 原实现拉回该对象全部修订实体再内存求 `max`；
/// 修订数随时间线性增长，改为 `revision_no` 降序 `limit 1` 投影查询，
/// 内存占用与传输量恒定；软删除过滤口径与基类 `find_many` 一致。
///
/// # 参数
/// * `revisions` - 修订集合（调用方经 `clone_with_type` 投影为修订号行）
/// * `owner_filter` - 归属过滤（如 `product_id`/`sku_id` 相等条件）
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回历史最大修订号；无修订时返回 `None`。
///
/// # 错误
/// 当 MongoDB 查询或反序列化失败时返回错误。
pub(super) async fn max_revision_no(
    revisions: &mongodb::Collection<RevisionNoRow>,
    owner_filter: Document,
    executor: &mut dyn Executor,
) -> Result<Option<u32>> {
    let mut filter = owner_filter;
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let row = match executor.session() {
        Some(session) => {
            revisions
                .find_one(filter)
                .projection(doc! { "revision_no": 1 })
                .sort(doc! { "revision_no": -1 })
                .session(session)
                .await?
        },
        None => {
            revisions
                .find_one(filter)
                .projection(doc! { "revision_no": 1 })
                .sort(doc! { "revision_no": -1 })
                .await?
        },
    };
    Ok(row.map(|row| row.revision_no))
}

/// 当前修订解析的泛型内核（erp-catalog-006）。
///
/// 商品与 SKU 的 `select_current_*_revision` 逻辑完全同构（优先当前修订指针，
/// 缺失回退最大修订号）；三处差异经访问器传入，两处各留一行特化委托。
///
/// # 参数
/// * `current_revision_id` - 稳定主表的当前修订指针
/// * `revisions` - 同一归属对象的修订集合
/// * `revision_id` - 取修订稳定 ID
/// * `revision_no` - 取修订号
///
/// # 返回
/// 优先返回当前指针命中的修订；否则返回最大修订号；无修订时返回 `None`。
pub(super) fn select_current_revision<'a, Revision>(
    current_revision_id: Option<&str>,
    revisions: &'a [Revision],
    revision_id: impl Fn(&Revision) -> &str,
    revision_no: impl Fn(&Revision) -> u32,
) -> Option<&'a Revision> {
    current_revision_id
        .and_then(|current_id| revisions.iter().find(|revision| revision_id(revision) == current_id))
        .or_else(|| revisions.iter().max_by_key(|revision| revision_no(revision)))
}

/// 分页投影查询的通用骨架（erp-catalog-004）。
///
/// 五个 `search_*`（商品修订、SKU、SKU 修订、品牌、计量单位）均为
/// “组装 FindOptions（排序/分页/投影）→find_many→count_documents→组装
/// PageResult”同一骨架，仅排序白名单函数与投影文档不同；各方法只保留
/// 排序与投影特化后委托至此。
///
/// # 参数
/// * `rows` - 行投影类型集合
/// * `entities` - 实体集合（仅用于计数）
/// * `filter` - 已组装的筛选条件
/// * `options` - 已组装的排序/分页/投影选项
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回当前页投影行与满足筛选条件的总数。
///
/// # 错误
/// 当 MongoDB 查询、游标读取或计数失败时返回错误。
pub(super) async fn search_projected<Row, Entity>(
    rows: &mongodb::Collection<Row>,
    entities: &mongodb::Collection<Entity>,
    filter: &impl QueryFilter,
    options: FindOptions,
    executor: &mut dyn Executor,
) -> Result<PageResult<Row>>
where
    Row: DeserializeOwned + Send + Sync,
    Entity: Send + Sync,
{
    let items = mongo_ops::find_many(rows, filter.to_doc(), options, executor).await?;
    let total = mongo_ops::count_documents(entities, filter.to_doc(), executor).await?;
    Ok(PageResult { items, total: total as i64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_doc_applies_direction() {
        assert_eq!(sort_doc("created_at", false), doc! { "created_at": -1, "id": -1 });
        assert_eq!(sort_doc("sku_no", true), doc! { "sku_no": 1, "id": 1 });
    }
}
