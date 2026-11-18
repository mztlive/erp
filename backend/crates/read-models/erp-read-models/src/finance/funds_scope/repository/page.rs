//! 同一最终条件的页面、总数、完整版本和轻量汇总分支。

use mongodb::Collection;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;
use serde::de::DeserializeOwned;

use crate::Result;

// 执行资金列表及其窄投影的聚合时，沿用调用方执行器，以目标行类型解码结果；不另开事务。
//
// 调用参数：
// * `collection` - 要聚合的集合。
// * `pipeline` - 已形成最终条件的管道。
// * `executor` - 调用方执行器；有会话时沿用该会话。
//
// 返回聚合投影结果。
//
// 聚合执行或类型化解码失败时返回对应错误。
pub(crate) async fn aggregate<T: DeserializeOwned + Send + Sync>(
    collection: Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    let rows =
        persistence_core::aggregate::rows(&collection.clone_with_type::<T>(), pipeline, executor).await?;
    Ok(rows)
}

// 资金列表从同一最终匹配条件生成页、计数、版本和汇总；只给 `items` 分支应用页面偏移和页大小。
//
// 调用参数：
// * `page` - 页码，从 1 起。
// * `size` - 页大小。
// * `sort` - 稳定排序。
// * `item_projection` - 当前页投影。
// * `version_projection` - 完整版本投影。
// * `summary_pipeline` - 汇总分支管道。
//
// 共享最终条件的 `$facet`。页偏移无法用 `i64` 表示时，页面分支是恒假匹配，版本与汇总分支仍完整。
pub(crate) fn page_facet(
    page: u64,
    size: u32,
    sort: Document,
    item_projection: Document,
    version_projection: Document,
    summary_pipeline: Vec<Document>,
) -> Result<Document> {
    let items = item_stages(page, size, sort.clone(), item_projection);
    Ok(doc! { "$facet": {
        "items": items,
        "total": [{ "$count": "count" }],
        "versions": [{ "$sort": sort }, { "$limit": 10001 }, { "$project": version_projection }],
        "summary": summary_pipeline,
    } })
}

// 资金列表需要用独立游标读取范围版本和金额时，聚合文档只打包当前页及总数。
//
// 调用参数：
// * `page` - 页码，从 1 起。
// * `size` - 页大小。
// * `sort` - 稳定排序。
// * `projection` - 当前页投影。
//
// 只含 `items` 与 `total` 的 `$facet`。页偏移无法用 `i64` 表示时，页面分支是恒假匹配。
pub(crate) fn page_only_facet(page: u64, size: u32, sort: Document, projection: Document) -> Document {
    doc! { "$facet": { "items": item_stages(page, size, sort, projection), "total": [{ "$count": "count" }] } }
}

// 为资金列表构造排序、偏移、大小与字段投影；深页偏移不适合 `i64` 时只将页面置空，不影响总数和范围版本。
fn item_stages(page: u64, size: u32, sort: Document, projection: Document) -> Vec<Document> {
    let offset =
        page.saturating_sub(1).checked_mul(u64::from(size)).and_then(|value| i64::try_from(value).ok());
    match offset {
        Some(offset) => vec![
            doc! { "$sort": sort },
            doc! { "$skip": offset },
            doc! { "$limit": i64::from(size) },
            doc! { "$project": projection },
        ],
        None => vec![doc! { "$match": { "$expr": false } }],
    }
}

// 构造资金列表的稳定排序：只接受领域白名单字段，未知字段回退到 `created_at`，并追加同方向的唯一 `id` 尾键。
//
// 调用参数：
// * `field` - 请求的排序字段。
// * `ascending` - 为 true 时升序。
// * `allowed` - 允许的排序字段。
//
// `field` 在白名单内时按其排序，否则按 `created_at`；同一方向追加 `id` 尾键。
pub(crate) fn sort_document(field: &str, ascending: bool, allowed: &[&str]) -> Document {
    let field = if allowed.contains(&field) { field } else { "created_at" };
    let direction = if ascending { 1 } else { -1 };
    doc! { field: direction, "id": direction }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepest_legal_page_keeps_complete_other_branches() {
        let facet = page_facet(
            u64::MAX,
            100,
            doc! { "id": 1 },
            doc! { "id": 1 },
            doc! { "version": 1 },
            vec![doc! { "$project": { "amount": 1 } }],
        )
        .unwrap();
        let branches = facet.get_document("$facet").unwrap();
        assert_eq!(branches.get_array("items").unwrap(), &vec![doc! { "$match": { "$expr": false } }.into()]);
        assert_eq!(branches.get_array("total").unwrap().len(), 1);
        assert_eq!(branches.get_array("versions").unwrap().len(), 3);
        assert_eq!(branches.get_array("summary").unwrap().len(), 1);

        let normal = page_only_facet(2, 25, doc! { "id": 1 }, doc! { "id": 1 });
        assert_eq!(
            normal.get_document("$facet").unwrap().get_array("items").unwrap()[1]
                .as_document()
                .unwrap()
                .get_i64("$skip")
                .unwrap(),
            25
        );
    }
}
