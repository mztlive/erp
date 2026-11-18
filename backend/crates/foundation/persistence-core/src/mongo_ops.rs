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
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::{FindOptions, ReturnDocument};
use mongodb::results::{DeleteResult, UpdateResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::Executor;
use crate::errors::Result;
use crate::executor::session;

/// 按执行器是否携带会话，展开 MongoDB 驱动的带/不带 `session` 调用。
///
/// 错误继续经 `?` 走 `Error::from(mongodb::error::Error)`，保持 DuplicateKey /
/// TransientTransactionConflict 分类。`find_many` 的 `SessionCursor::next` 还须再借
/// 一次会话，不得使用本宏。
macro_rules! exec_with_session {
    ($executor:expr, $operation:expr) => {
        match session($executor) {
            Some(session) => $operation.session(session).await?,
            None => $operation.await?,
        }
    };
}

/// 用调用方执行器插入一个文档，加入已有会话或独立提交；尝试写入前撤销旧读取阶段。
/// 唯一索引冲突等数据库错误按持久化分类返回。
pub fn insert_one<'a, T>(
    collection: &'a Collection<T>,
    document: &'a T,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>>
where
    T: Serialize + Send + Sync + 'a,
{
    Box::pin(async move {
        executor.next_read();
        exec_with_session!(executor, collection.insert_one(document));
        Ok(())
    })
}

/// 批量入库时按输入顺序有序插入；接收自有集合或借用切片，无需克隆实体。空输入不访问执行器，也不推进读取阶段；
/// 非空则在尝试写入前推进，并返回首个驱动错误。
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

        executor.next_read();
        exec_with_session!(executor, collection.insert_many(documents).ordered(true));
        Ok(())
    })
}

/// 需要核对单文档修改或乐观锁命中时，在原执行器内更新并读取 `UpdateResult`。
/// 仅显式启用 `upsert` 才能在未命中时插入；尝试前先废止旧读取阶段。
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
        executor.next_read();
        let result = exec_with_session!(executor, collection.update_one(filter, update).upsert(upsert));
        Ok(result)
    })
}

/// 将同一更新应用于全部命中文档，返回匹配与修改计数供批处理核对；沿用当前执行器，写入尝试前结束旧读取阶段。
pub fn update_many<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    update: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<UpdateResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        executor.next_read();
        let result = exec_with_session!(executor, collection.update_many(filter, update));
        Ok(result)
    })
}

/// 批量依赖文档现值计算新字段时，提交领域构造的过滤和更新管道，并返回匹配计数供核对完整 CAS。
/// 沿用原会话，尝试前撤销旧阶段，保留数据库错误分类。
pub fn update_many_pipeline<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    pipeline: Vec<Document>,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<UpdateResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        executor.next_read();
        let result = exec_with_session!(executor, collection.update_many(filter, pipeline));
        Ok(result)
    })
}

/// 当前字段必须同时生成历史事实、推进版本等场景，可用管道原子计算并返回更新后的文档。领域负责过滤与管道；
/// 沿用调用方执行器，写入尝试前撤销旧读取阶段，未匹配返回 `None`。
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
        executor.next_read();
        let operation =
            collection.find_one_and_update(filter, pipeline).return_document(ReturnDocument::After);
        let document = exec_with_session!(executor, operation);
        Ok(document)
    })
}

/// 清理一批匹配记录时执行物理删除并返回删除计数；沿用原执行器，尝试前结束旧读取阶段，不提供软删。
pub fn delete_many<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<DeleteResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        executor.next_read();
        let result = exec_with_session!(executor, collection.delete_many(filter));
        Ok(result)
    })
}

/// 仅需物理删除一条匹配记录时使用，调用方可凭删除计数识别未命中；沿用原执行器，写入尝试前撤销旧凭证。
pub fn delete_one<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<DeleteResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        executor.next_read();
        let result = exec_with_session!(executor, collection.delete_one(filter));
        Ok(result)
    })
}

/// 用本次读取的执行器取第一条匹配记录；不改变读取阶段，无匹配返回 `None`，查询或反序列化错误照常上报。
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

/// 按过滤及排序、分页等选项取完本次游标。若有会话，每次游标读取都继续借用同一会话；
/// 任何查询、取数或解码错误均返回，不交付部分结果。
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
        match session(executor) {
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

/// 只判断条件是否命中时投影 `_id` 并在首条停止，不载入完整业务文档。
/// 独立解码类型不要求实体字段或某种特定 `_id` 类型，读取沿用原阶段。
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

/// 用与列表或查询相同的过滤条件及执行器统计命中总数，不载入或解码完整文档。
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

/// 需要字段候选值时在原读取阶段做去重，不暴露或撤销事务边界；结果保留 BSON 类型，读取失败返回持久化错误。
pub fn distinct<'a, T>(
    collection: &'a Collection<T>,
    field: &'a str,
    filter: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<Vec<Bson>>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move { Ok(exec_with_session!(executor, collection.distinct(field, filter))) })
}

/// 从多个匹配候选中按领域指定顺序原子选取并更新第一条，返回更新后的文档，未命中返回 `None`。
/// 继续使用原执行器，但写入尝试前结束旧读取阶段。
pub fn find_one_and_update_sorted_pipeline<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    pipeline: Vec<Document>,
    sort: Document,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<Option<T>>>
where
    T: DeserializeOwned + Send + Sync + 'a,
{
    Box::pin(async move {
        executor.next_read();
        let operation = collection
            .find_one_and_update(filter, pipeline)
            .sort(sort)
            .return_document(ReturnDocument::After);
        Ok(exec_with_session!(executor, operation))
    })
}

/// 用原执行器为单文档执行基于当前字段的更新管道，保留匹配与修改计数；
/// 写入尝试前结束旧读取阶段，驱动错误按持久化分类返回。
pub fn update_one_pipeline<'a, T>(
    collection: &'a Collection<T>,
    filter: Document,
    pipeline: Vec<Document>,
    executor: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<UpdateResult>>
where
    T: Send + Sync + 'a,
{
    Box::pin(async move {
        executor.next_read();
        Ok(exec_with_session!(executor, collection.update_one(filter, pipeline)))
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
    use crate::errors::Error;
    use crate::executor::sealed::Sealed;
    use crate::{NoTransaction, ReadExecutor};

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

    impl Sealed for UnusedExecutor {}

    impl Executor for UnusedExecutor {
        /// 空输入若触发会话访问则使测试失败。
        fn session(&mut self) -> Option<&mut ClientSession> {
            panic!("empty input must not access a session")
        }
    }

    // 验证空的自有集合与借用切片都绕过数据库和会话，不为无需插入的批次要求文档实现 `Clone`。
    #[tokio::test]
    async fn insert_many_accepts_empty_owned_and_borrowed_non_clone_documents() {
        let collection = traced_collection();
        let documents = Vec::<TracedDocument>::new();
        insert_many(&collection, documents.as_slice(), &mut UnusedExecutor).await.unwrap();
        insert_many(&collection, documents, &mut UnusedExecutor).await.unwrap();
    }

    // 用不可克隆的文档比较两种输入的有序序列化：遇到第二项错误立即停止，且错误文案和仓储分类一致。
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

    #[tokio::test]
    async fn attempted_write_invalidates_old_read_even_when_serialization_fails() {
        let collection = traced_collection();
        let mut executor = ReadExecutor::default();
        let before = executor.boundary().unwrap();

        insert_many(&collection, Vec::<TracedDocument>::new(), &mut executor).await.unwrap();
        assert!(before.is_current(&executor));

        let document = TracedDocument { id: 1, visits: Arc::default(), fail: true };
        let error = insert_many(&collection, vec![document], &mut executor).await.unwrap_err();
        assert!(matches!(error, Error::DatabaseError(_)));
        assert!(!before.is_current(&executor));
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
