//! 会话感知的 MongoDB 操作封装
//!
//! MongoDB 驱动对“带会话”和“不带会话”提供了两套调用形态：前者需要显式传入
//! `&mut ClientSession`，`find` 还会返回类型不同的 `SessionCursor`。本模块把这层分支
//! 收敛到一处，使 Repository 只面向 [`Executor`](crate::Executor) 编写一份实现。

use futures_util::StreamExt;
use mongodb::{
    bson::{doc, Document},
    options::FindOptions,
    results::{DeleteResult, UpdateResult},
    Collection,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::{errors::Result, Executor};

/// 按执行器语义插入单个文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `document` - 待插入文档
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 插入成功时返回 `Ok(())`。
///
/// # 错误
/// 当唯一索引冲突或 MongoDB 写入失败时返回错误。
pub(crate) async fn insert_one<T>(
    collection: &Collection<T>,
    document: &T,
    executor: &mut dyn Executor,
) -> Result<()>
where
    T: Serialize + Send + Sync,
{
    match executor.session() {
        Some(session) => collection.insert_one(document).session(session).await?,
        None => collection.insert_one(document).await?,
    };
    Ok(())
}

/// 按执行器语义批量插入文档。
///
/// 文档集合为空时直接返回，避免向 MongoDB 发送空批量写入命令。
///
/// # 参数
/// * `collection` - 目标集合
/// * `documents` - 待插入文档集合
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 插入成功时返回 `Ok(())`。
///
/// # 错误
/// 当唯一索引冲突或 MongoDB 写入失败时返回错误。
pub(crate) async fn insert_many<T>(
    collection: &Collection<T>,
    documents: Vec<T>,
    executor: &mut dyn Executor,
) -> Result<()>
where
    T: Serialize + Send + Sync,
{
    if documents.is_empty() {
        return Ok(());
    }

    match executor.session() {
        Some(session) => collection.insert_many(documents).session(session).await?,
        None => collection.insert_many(documents).await?,
    };
    Ok(())
}

/// 按执行器语义更新单个文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 更新条件
/// * `update` - 更新内容
/// * `upsert` - 条件未命中时是否插入新文档
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 返回 MongoDB 更新结果，调用方据此判断乐观锁是否命中。
///
/// # 错误
/// 当 MongoDB 更新失败时返回错误。
pub(crate) async fn update_one<T>(
    collection: &Collection<T>,
    filter: Document,
    update: Document,
    upsert: bool,
    executor: &mut dyn Executor,
) -> Result<UpdateResult>
where
    T: Send + Sync,
{
    let result = match executor.session() {
        Some(session) => {
            collection
                .update_one(filter, update)
                .upsert(upsert)
                .session(session)
                .await?
        }
        None => collection.update_one(filter, update).upsert(upsert).await?,
    };
    Ok(result)
}

/// 按执行器语义删除符合条件的全部文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 删除条件
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 返回 MongoDB 删除结果，调用方据此判断是否确有数据被删除。
///
/// # 错误
/// 当 MongoDB 删除失败时返回错误。
pub(crate) async fn delete_many<T>(
    collection: &Collection<T>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<DeleteResult>
where
    T: Send + Sync,
{
    let result = match executor.session() {
        Some(session) => collection.delete_many(filter).session(session).await?,
        None => collection.delete_many(filter).await?,
    };
    Ok(result)
}

/// 按执行器语义删除符合条件的单个文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 删除条件
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 返回 MongoDB 删除结果，调用方据此判断是否确有数据被删除。
///
/// # 错误
/// 当 MongoDB 删除失败时返回错误。
pub(crate) async fn delete_one<T>(
    collection: &Collection<T>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<DeleteResult>
where
    T: Send + Sync,
{
    let result = match executor.session() {
        Some(session) => collection.delete_one(filter).session(session).await?,
        None => collection.delete_one(filter).await?,
    };
    Ok(result)
}

/// 按执行器语义查询单个文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 查询条件
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 返回匹配的第一个文档；无匹配时返回 `None`。
///
/// # 错误
/// 当 MongoDB 查询或反序列化失败时返回错误。
pub(crate) async fn find_one<T>(
    collection: &Collection<T>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<Option<T>>
where
    T: DeserializeOwned + Send + Sync,
{
    let document = match executor.session() {
        Some(session) => collection.find_one(filter).session(session).await?,
        None => collection.find_one(filter).await?,
    };
    Ok(document)
}

/// 按执行器语义查询多个文档。
///
/// 带会话时使用 `SessionCursor` 逐条读取，与非会话游标返回同样的集合结果。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 查询条件
/// * `options` - 排序与分页等查询选项
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 返回全部匹配文档。
///
/// # 错误
/// 当 MongoDB 查询、游标读取或反序列化失败时返回错误。
pub(crate) async fn find_many<T>(
    collection: &Collection<T>,
    filter: Document,
    options: FindOptions,
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: DeserializeOwned + Send + Sync,
{
    let mut documents = Vec::new();
    match executor.session() {
        Some(session) => {
            let mut cursor = collection
                .find(filter)
                .with_options(options)
                .session(&mut *session)
                .await?;
            while let Some(document) = cursor.next(&mut *session).await.transpose()? {
                documents.push(document);
            }
        }
        None => {
            let mut cursor = collection.find(filter).with_options(options).await?;
            while let Some(document) = cursor.next().await {
                documents.push(document?);
            }
        }
    }
    Ok(documents)
}

/// 按执行器语义判断是否存在符合条件的文档。
///
/// 查询只投影 MongoDB `_id` 并在首条命中后停止，避免为存在性判断加载完整文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 查询条件
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 存在匹配文档时返回 `true`。
///
/// # 错误
/// 当 MongoDB 查询失败时返回错误。
pub(crate) async fn exists(
    collection: &Collection<Document>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let projection = doc! { "_id": 1 };
    let document = match executor.session() {
        Some(session) => {
            collection
                .find_one(filter)
                .projection(projection)
                .session(session)
                .await?
        }
        None => collection.find_one(filter).projection(projection).await?,
    };
    Ok(document.is_some())
}

/// 按执行器语义统计符合条件的文档数量。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 统计条件
/// * `executor` - 数据访问执行器
///
/// # 返回值
/// 返回匹配文档数量。
///
/// # 错误
/// 当 MongoDB 统计失败时返回错误。
pub(crate) async fn count_documents<T>(
    collection: &Collection<T>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<u64>
where
    T: Send + Sync,
{
    let count = match executor.session() {
        Some(session) => collection.count_documents(filter).session(session).await?,
        None => collection.count_documents(filter).await?,
    };
    Ok(count)
}
