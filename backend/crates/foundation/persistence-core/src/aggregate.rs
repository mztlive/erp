//! 带调用方执行器的聚合读取；阶段顺序由拥有集合的仓储确定。
mod cursor;
pub use cursor::RowCursor;
use futures_util::TryStreamExt;
use mongodb::Collection;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::executor::session;
use crate::{Executor, Result};

/// 用调用方的执行器运行仓储组装的完整聚合管道，收集并解码所有结果。事务游标继续借原会话，不结束当前读取阶段；
/// 查询、取数或解码失败向上返回。
pub async fn rows<T: DeserializeOwned + Send + Sync>(
    collection: &Collection<T>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    let operation = collection.aggregate(pipeline).with_type::<T>();
    Ok(match session(executor) {
        Some(session) => operation.session(&mut *session).await?.stream(session).try_collect().await?,
        None => operation.await?.try_collect().await?,
    })
}

/// 在仓储提供的关联阶段之后依次追加筛选、排序、分页和投影，让过滤看到关联后的数据且只分页最终匹配项。
/// 沿用本次读取执行器；偏移超出 `i64` 或查询、解码失败时返回错误。
pub async fn find<T: DeserializeOwned + Send + Sync>(
    collection: &Collection<T>,
    filter: Document,
    options: FindOptions,
    mut prefix: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    prefix.push(doc! { "$match": filter });
    if let Some(sort) = options.sort {
        prefix.push(doc! { "$sort": sort });
    }
    if let Some(skip) = options.skip.filter(|v| *v > 0) {
        let skip = i64::try_from(skip).map_err(|_| crate::Error::EntityMetadataOutOfRange("分页偏移超限"))?;
        prefix.push(doc! { "$skip": skip });
    }
    if let Some(limit) = options.limit.filter(|v| *v > 0) {
        prefix.push(doc! { "$limit": limit });
    }
    if let Some(projection) = options.projection {
        prefix.push(doc! { "$project": projection });
    }

    rows(collection, prefix, executor).await
}

#[derive(Deserialize)]
struct Count {
    count: u64,
}

/// 为关联查询的分页统计复用同一前置管道和最终筛选，不应用分页；用原执行器计数，没有结果时返回零。
pub async fn count<T: Send + Sync>(
    collection: &Collection<T>,
    filter: Document,
    mut prefix: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<u64> {
    prefix.extend([doc! { "$match": filter }, doc! { "$count": "count" }]);
    Ok(rows(&collection.clone_with_type::<Count>(), prefix, executor).await?.first().map_or(0, |r| r.count))
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{deserialize_from_slice, doc, serialize_to_vec};

    use super::Count;

    #[test]
    fn aggregate_count_decodes_both_bson_integer_widths() {
        let cases = [(doc! { "count": 1_i32 }, 1), (doc! { "count": 2_147_483_648_i64 }, 2_147_483_648)];
        for (row, expected) in cases {
            let count = deserialize_from_slice::<Count>(&serialize_to_vec(&row).unwrap()).unwrap();
            assert_eq!(count.count, expected);
        }
    }

    #[test]
    fn aggregate_count_rejects_invalid_values() {
        for row in [doc! {}, doc! { "count": -1_i32 }, doc! { "count": "1" }] {
            assert!(deserialize_from_slice::<Count>(&serialize_to_vec(&row).unwrap()).is_err());
        }
    }
}
