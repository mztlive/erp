//! 同一最终条件的页面、总数、完整版本和轻量汇总分支。

use futures_util::TryStreamExt;
use mongodb::Collection;
use mongodb::bson::{Document, doc};
use persistence_core::{Error as PersistenceError, Executor};
use serde::de::DeserializeOwned;

use crate::Result;

/// 使用原执行器执行类型化只读聚合，不另开事务。
///
/// # 参数
/// 集合句柄、已形成最终条件的管道和原执行器。
/// # 返回
/// 返回聚合投影结果。
/// # 错误
/// MongoDB 或类型化解码失败时拒绝。
pub(crate) async fn aggregate<T: DeserializeOwned + Send + Sync>(
    collection: Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    let rows = match executor.session() {
        Some(session) => collection
            .aggregate(pipeline)
            .with_type::<T>()
            .session(&mut *session)
            .await
            .map_err(PersistenceError::from)?
            .stream(session)
            .try_collect()
            .await
            .map_err(PersistenceError::from)?,
        None => collection
            .aggregate(pipeline)
            .with_type::<T>()
            .await
            .map_err(PersistenceError::from)?
            .try_collect()
            .await
            .map_err(PersistenceError::from)?,
    };
    Ok(rows)
}

/// 分页只作用于实体页；版本和汇总仍覆盖全部最终匹配记录。
///
/// # 参数
/// 页码、条数、稳定排序、页面/版本投影及汇总管道。
/// # 返回
/// 返回共享最终条件的 `$facet`。
/// # 错误
/// 无；超过数据库整数范围的合法深页返回空页，其余分支仍完整。
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

/// 只将当前页和总数压入单文档，完整版本及金额使用独立游标。
pub(crate) fn page_only_facet(page: u64, size: u32, sort: Document, projection: Document) -> Document {
    doc! { "$facet": { "items": item_stages(page, size, sort, projection), "total": [{ "$count": "count" }] } }
}

/// 深页整数无法表示时保持旧内存分页的空页合同，不影响总数与范围版本。
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

/// 与领域列表保持相同白名单和唯一 ID 尾键排序。
///
/// # 参数
/// 已规范化字段、方向和领域允许字段。
/// # 返回
/// 返回稳定排序条件。
/// # 错误
/// 无。
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
