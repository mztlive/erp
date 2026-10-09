//! 会话感知的 MongoDB 操作封装
//!
//! MongoDB 驱动对“带会话”和“不带会话”提供了两套调用形态：前者需要显式传入
//! `&mut ClientSession`，`find` 还会返回类型不同的 `SessionCursor`。本模块把这层分支
//! 收敛到一处，使 Repository 只面向 [`Executor`](crate::Executor) 编写一份实现。
//!
//! 各操作返回装箱的 `BoxFuture` 而非 `async fn`：驱动内部的游标、会话等类型不再泄漏到
//! 调用方 Future 中，上层 Port 实现证明 `Send` 时不必逐层展开驱动类型（erp-processes
//! 全量编译实测减少约 20%）。改回 `async fn` 前须重新评估编译耗时。

use std::borrow::Borrow;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use mongodb::Collection;
use mongodb::bson::{Document, doc};
use mongodb::options::{FindOptions, ReturnDocument};
use mongodb::results::{DeleteResult, UpdateResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::Executor;
use crate::errors::Result;

/// 按执行器是否携带会话，展开 MongoDB 驱动的带/不带 `session` 调用。
///
/// 错误继续经 `?` 走 `Error::from(mongodb::error::Error)`，保持 DuplicateKey /
/// TransientTransactionConflict 分类。`find_many` 的 `SessionCursor::next` 还须再借
/// 一次会话，不得使用本宏。
macro_rules! exec_with_session {
    ($executor:expr, $operation:expr) => {
        match $executor.session() {
            Some(session) => $operation.session(session).await?,
            None => $operation.await?,
        }
    };
}

/// 按执行器语义插入单个文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `document` - 待插入文档
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 插入成功时返回 `Ok(())`。
///
/// # 错误
/// 当唯一索引冲突或 MongoDB 写入失败时返回错误。
pub fn insert_one<'a, T>(
    collection: &'a Collection<T>,
    document: &'a T,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>>
where
    T: Serialize + Send + Sync + 'a,
{
    Box::pin(async move {
        exec_with_session!(executor, collection.insert_one(document));
        Ok(())
    })
}

/// 按执行器语义批量插入文档。
///
/// 文档集合为空时直接返回；拥有型集合与借用切片均直接交给驱动序列化，
/// 不要求克隆业务实体。输入顺序及驱动默认的有序插入语义保持不变。
///
/// # 参数
/// * `collection` - 目标集合
/// * `documents` - 待插入文档集合或借用切片
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 插入成功时返回 `Ok(())`。
///
/// # 错误
/// 当唯一索引冲突或 MongoDB 写入失败时返回错误。
pub fn insert_many<'a, T, D>(
    collection: &'a Collection<T>,
    documents: D,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>>
where
    T: Serialize + Send + Sync + 'a,
    D: IntoIterator + Send + 'a,
    D::IntoIter: ExactSizeIterator + Send + 'a,
    D::Item: Borrow<T> + Send + 'a,
{
    Box::pin(async move {
        let documents = documents.into_iter();
        if documents.len() == 0 {
            return Ok(());
        }

        exec_with_session!(executor, collection.insert_many(documents));
        Ok(())
    })
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
/// # 返回
/// 返回 MongoDB 更新结果，调用方据此判断乐观锁是否命中。
///
/// # 错误
/// 当 MongoDB 更新失败时返回错误。
pub fn update_one<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    update: Document,
    upsert: bool,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<UpdateResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        let result = exec_with_session!(executor, collection.update_one(filter, update).upsert(upsert));
        Ok(result)
    })
}

/// 按执行器语义执行单文档更新管道，并返回更新后的文档。
///
/// 该入口用于必须由 MongoDB 原子计算的写入，例如依赖当前字段同时形成历史
/// 事实并推进版本。Repository 负责组装业务过滤条件和更新管道；事务边界仍由
/// 调用方传入的执行器决定。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 更新前置条件
/// * `pipeline` - MongoDB 聚合更新管道。
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回更新后的文档；条件未命中时返回 `None`。
///
/// # 错误
/// 当 MongoDB 更新或文档反序列化失败时返回错误。
pub fn find_one_and_update_pipeline<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    pipeline: Vec<Document>,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<Option<T>>>
where
    T: DeserializeOwned + Send + Sync + 'a,
{
    Box::pin(async move {
        let operation =
            collection.find_one_and_update(filter, pipeline).return_document(ReturnDocument::After);
        let document = exec_with_session!(executor, operation);
        Ok(document)
    })
}

/// 按执行器语义删除符合条件的全部文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 删除条件
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回 MongoDB 删除结果，调用方据此判断是否确有数据被删除。
///
/// # 错误
/// 当 MongoDB 删除失败时返回错误。
pub fn delete_many<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<DeleteResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        let result = exec_with_session!(executor, collection.delete_many(filter));
        Ok(result)
    })
}

/// 按执行器语义删除符合条件的单个文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 删除条件
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回 MongoDB 删除结果，调用方据此判断是否确有数据被删除。
///
/// # 错误
/// 当 MongoDB 删除失败时返回错误。
pub fn delete_one<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<DeleteResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        let result = exec_with_session!(executor, collection.delete_one(filter));
        Ok(result)
    })
}

/// 按执行器语义查询单个文档。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 查询条件
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回匹配的第一个文档；无匹配时返回 `None`。
///
/// # 错误
/// 当 MongoDB 查询或反序列化失败时返回错误。
pub fn find_one<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<Option<T>>>
where
    T: DeserializeOwned + Send + Sync + 'a,
{
    Box::pin(async move {
        let document = exec_with_session!(executor, collection.find_one(filter));
        Ok(document)
    })
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
/// # 返回
/// 返回全部匹配文档。
///
/// # 错误
/// 当 MongoDB 查询、游标读取或反序列化失败时返回错误。
pub fn find_many<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    options: FindOptions,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<Vec<T>>>
where
    T: DeserializeOwned + Send + Sync + 'a,
{
    Box::pin(async move {
        let mut documents = Vec::new();
        match executor.session() {
            Some(session) => {
                let mut cursor = collection.find(filter).with_options(options).session(&mut *session).await?;
                while let Some(document) = cursor.next(&mut *session).await.transpose()? {
                    documents.push(document);
                }
            },
            None => {
                let mut cursor = collection.find(filter).with_options(options).await?;
                while let Some(document) = cursor.next().await.transpose()? {
                    documents.push(document);
                }
            },
        }
        Ok(documents)
    })
}

/// 按执行器语义判断是否存在符合条件的文档。
///
/// 查询只投影 MongoDB `_id` 并在首条命中后停止，避免为存在性判断加载完整文档。
/// 投影使用独立结果类型，不能按缺少业务必填字段的完整实体反序列化。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 查询条件
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 存在匹配文档时返回 `true`，否则返回 `false`。
///
/// # 错误
/// 当 MongoDB 查询失败时返回错误。
pub fn exists<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<bool>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        let collection = collection.clone_with_type::<ExistsDocument>();
        let projection = doc! { "_id": 1 };
        let document = exec_with_session!(executor, collection.find_one(filter).projection(projection));
        Ok(document.is_some())
    })
}

/// 存在性查询只关心是否命中；忽略 `_id` 的存储类型及所有业务字段。
#[derive(Debug, Deserialize)]
struct ExistsDocument {}

/// 按执行器语义统计符合条件的文档数量。
///
/// # 参数
/// * `collection` - 目标集合
/// * `filter` - 统计条件
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回匹配文档数量。
///
/// # 错误
/// 当 MongoDB 统计失败时返回错误。
pub fn count_documents<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<u64>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        let count = exec_with_session!(executor, collection.count_documents(filter));
        Ok(count)
    })
}

#[cfg(test)]
mod tests {
    use std::result::Result as SerializationResult;
    use std::sync::{Arc, Mutex};

    use mongodb::bson::oid::ObjectId;
    use mongodb::bson::{Bson, deserialize_from_document};
    use mongodb::options::{ClientOptions, ServerAddress};
    use mongodb::{Client, ClientSession};
    use serde::Serializer;
    use serde::ser::Error as SerializeError;

    use super::*;
    use crate::NoTransaction;
    use crate::errors::Error;

    /// 不实现 Clone 的文档；记录序列化顺序并在指定位置终止。
    struct TracedDocument {
        id: u32,
        visits: Arc<Mutex<Vec<u32>>>,
        fail: bool,
    }

    impl Serialize for TracedDocument {
        /// 执行真实驱动调用的序列化，并返回可识别的首个错误。
        fn serialize<S: Serializer>(&self, serializer: S) -> SerializationResult<S::Ok, S::Error> {
            self.visits.lock().unwrap().push(self.id);
            if self.fail {
                return Err(S::Error::custom(format!("document-{}-failed", self.id)));
            }
            doc! { "_id": self.id }.serialize(serializer)
        }
    }

    /// 构造驱动集合句柄；测试在空集合或序列化阶段返回，不执行数据库操作。
    fn traced_collection() -> Collection<TracedDocument> {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        Client::with_options(options).unwrap().database("unit_test").collection("documents")
    }

    /// 空输入不得访问执行器会话。
    struct UnusedExecutor;

    impl Executor for UnusedExecutor {
        /// 空输入若触发会话访问则使测试失败。
        fn session(&mut self) -> Option<&mut ClientSession> {
            panic!("empty input must not access a session")
        }
    }

    /// 拥有型及借用型空集合都直接成功，不调用执行器。
    #[tokio::test]
    async fn insert_many_accepts_empty_owned_and_borrowed_non_clone_documents() {
        let collection = traced_collection();
        let documents = Vec::<TracedDocument>::new();
        insert_many(&collection, documents.as_slice(), &mut UnusedExecutor).await.unwrap();
        insert_many(&collection, documents, &mut UnusedExecutor).await.unwrap();
    }

    /// 两种输入均保持序列化顺序、首个错误及数据库错误分类，不要求实体 Clone。
    #[tokio::test]
    async fn insert_many_preserves_first_serialization_error_for_owned_and_borrowed_inputs() {
        let collection = traced_collection();
        let visits = Arc::new(Mutex::new(Vec::new()));
        let documents = (1..=3)
            .map(|id| TracedDocument { id, visits: visits.clone(), fail: id == 2 })
            .collect::<Vec<_>>();
        let borrowed = insert_many(&collection, documents.as_slice(), &mut NoTransaction).await.unwrap_err();
        assert!(matches!(borrowed, Error::DatabaseError(_)));
        assert!(borrowed.to_string().contains("document-2-failed"));
        assert_eq!(*visits.lock().unwrap(), [1, 2]);
        visits.lock().unwrap().clear();
        let owned = insert_many(&collection, documents, &mut NoTransaction).await.unwrap_err();
        assert!(matches!(owned, Error::DatabaseError(_)));
        assert_eq!(owned.to_string(), borrowed.to_string());
        assert_eq!(*visits.lock().unwrap(), [1, 2]);
    }

    #[derive(Debug, Deserialize)]
    struct NamedEntity {
        #[serde(rename = "name")]
        _name: String,
    }

    #[test]
    fn existence_projection_does_not_require_business_fields() {
        let projected = doc! { "_id": ObjectId::new() };
        let error = deserialize_from_document::<NamedEntity>(projected.clone()).unwrap_err();
        assert!(error.to_string().contains("missing field `name`"));
        // 使用生产 exists 查询的实际解码类型，命中记录不再要求 name 等业务字段。
        deserialize_from_document::<ExistsDocument>(projected).unwrap();
    }

    #[test]
    fn existence_projection_accepts_different_mongodb_id_types() {
        for id in [Bson::ObjectId(ObjectId::new()), Bson::String("role-root".into()), Bson::Int64(7)] {
            deserialize_from_document::<ExistsDocument>(doc! { "_id": id }).unwrap();
        }
    }
}
