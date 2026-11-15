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

/// 执行聚合并解码目标投影。
/// # 参数
/// collection 为集合；pipeline 为完整管道；executor 为当前事务。
/// # 返回
/// 数据库聚合结果。
/// # 错误
/// 查询或解码失败时返回错误。
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

/// 先执行关联和筛选，再执行排序、分页和投影。
/// # 参数
/// collection、filter、options 为仓储查询；prefix 为筛选前关联阶段；executor 为原事务。
/// # 返回
/// 最终条件内的结果。
/// # 错误
/// 数值超限、数据库或解码失败返回错误。
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

/// 在完整关联和筛选后计数。
/// # 参数
/// collection 为集合；filter 为最终条件；prefix 为关联阶段；executor 为原事务。
/// # 返回
/// 全部匹配记录数量。
/// # 错误
/// 数据库或解码失败返回错误。
pub async fn count<T: Send + Sync>(
    collection: &Collection<T>,
    filter: Document,
    mut prefix: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<u64> {
    #[derive(Deserialize)]
    struct Count {
        count: u64,
    }
    prefix.extend([doc! { "$match": filter }, doc! { "$count": "count" }]);
    Ok(rows(&collection.clone_with_type::<Count>(), prefix, executor).await?.first().map_or(0, |r| r.count))
}
