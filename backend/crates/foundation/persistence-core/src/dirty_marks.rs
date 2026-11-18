//! 派生投影的待重算标记：以 `_id` 为业务主键，`dirty` 表示待重算，`dirty_token` 每次标记递增。
//!
//! 后台读取一批待重算记录及其标记序号，计算后只在序号未变时清除标记，计算期间的
//! 新标记不会被覆盖。标记与保存都可重入，多实例并发只会重复计算。

use std::collections::BTreeSet;

use mongodb::Collection;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use serde::Deserialize;

use crate::errors::{Error, Result};
use crate::{Executor, mongo_ops};

/// 单次 `$in` 标记的主键数量上限。
const MARK_CHUNK: usize = 500;

/// 待重算记录及读取时的标记序号。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DirtyMark {
    /// 记录主键。
    #[serde(rename = "_id")]
    pub id: String,
    /// 读取时的标记序号。
    pub dirty_token: i64,
}

#[derive(Deserialize)]
struct IdRow {
    #[serde(rename = "_id")]
    id: String,
}

/// 为已有投影构造待重算更新：置 `dirty` 并递增 `dirty_token`，使计算中的旧结果无法清除新标记。
pub fn mark_update() -> Document {
    doc! { "$set": { "dirty": true }, "$inc": { "dirty_token": 1_i64 } }
}

/// 源数据变化后按投影主键去重、分批标脏；无投影的主键先插待算占位。
/// 占位构造器只提供业务字段，不自行管理 `_id`、`dirty` 和 `dirty_token`；空输入不读写数据库。
pub async fn mark_ids(
    collection: &Collection<Document>,
    ids: &[String],
    pending: impl Fn(&str) -> Document,
    executor: &mut dyn Executor,
) -> Result<()> {
    let unique = ids.iter().map(String::as_str).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();

    for chunk in unique.chunks(MARK_CHUNK) {
        let filter = doc! { "_id": { "$in": chunk } };
        mongo_ops::update_many(collection, filter.clone(), mark_update(), executor).await?;

        let rows = collection.clone_with_type::<IdRow>();
        let options = FindOptions::builder().projection(doc! { "_id": 1 }).build();
        let existing = mongo_ops::find_many(&rows, filter, options, executor).await?;
        let existing = existing.into_iter().map(|row| row.id).collect::<BTreeSet<_>>();

        let missing = chunk.iter().copied().filter(|id| !existing.contains(*id)).collect::<Vec<_>>();
        insert_pending(collection, &missing, &pending, executor).await?;
    }
    Ok(())
}

// 为未出现的主键批量加入待算占位。并发插入触发唯一冲突时逐键 upsert，使被其他实例抢先插入的记录也标为待算。
async fn insert_pending(
    collection: &Collection<Document>,
    ids: &[&str],
    pending: &impl Fn(&str) -> Document,
    executor: &mut dyn Executor,
) -> Result<()> {
    let documents = ids
        .iter()
        .map(|id| {
            let mut document = pending(id);
            document.insert("_id", *id);
            document.insert("dirty", true);
            document.insert("dirty_token", 1_i64);
            document
        })
        .collect::<Vec<_>>();

    match mongo_ops::insert_many(collection, &documents, executor).await {
        Err(Error::DuplicateKey(_)) => {},
        other => return other,
    }

    for id in ids {
        let mut update = mark_update();
        update.insert("$setOnInsert", pending(id));
        mongo_ops::update_one(collection, doc! { "_id": *id }, update, true, executor).await?;
    }
    Ok(())
}

/// 投影已存在且可用同一条件定位时，批量递增标记序号并返回修改条数；不为未命中的主键补占位。
pub async fn mark_matching(
    collection: &Collection<Document>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<u64> {
    Ok(mongo_ops::update_many(collection, filter, mark_update(), executor).await?.modified_count)
}

/// 后台开始重算前，按给定 MongoDB `limit` 读取待处理主键及当时的标记序号；写回时用该序号检查计算期间是否有更新。
pub async fn dirty_batch(
    collection: &Collection<Document>,
    limit: i64,
    executor: &mut dyn Executor,
) -> Result<Vec<DirtyMark>> {
    let options = FindOptions::builder().projection(doc! { "_id": 1, "dirty_token": 1 }).limit(limit).build();
    let marks = collection.clone_with_type::<DirtyMark>();
    mongo_ops::find_many(&marks, doc! { "dirty": true }, options, executor).await
}

/// 统计投影集合当前仍标记待重算的条数，供后台判断积压，不把已完成项计入。
pub async fn dirty_count(collection: &Collection<Document>, executor: &mut dyn Executor) -> Result<u64> {
    mongo_ops::count_documents(collection, doc! { "dirty": true }, executor).await
}

/// 只在主键与重算前的标记序号仍匹配时写回结果并清除 `dirty`。计算字段不得覆盖主键和标记字段；
/// 未命中返回 `false`，避免覆盖计算期间的新标记。
pub async fn save_clean(
    collection: &Collection<Document>,
    id: &str,
    dirty_token: i64,
    mut fields: Document,
    executor: &mut dyn Executor,
) -> Result<bool> {
    fields.insert("dirty", false);
    let filter = doc! { "_id": id, "dirty_token": dirty_token };
    let result = mongo_ops::update_one(collection, filter, doc! { "$set": fields }, false, executor).await?;
    Ok(result.matched_count == 1)
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use mongodb::options::{ClientOptions, ServerAddress};
    use mongodb::{Client, ClientSession};

    use super::{mark_ids, mark_update};
    use crate::Executor;
    use crate::executor::sealed::Sealed;

    struct UnusedExecutor;

    impl Sealed for UnusedExecutor {}

    impl Executor for UnusedExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            panic!("empty IDs must not access an executor session")
        }
    }

    #[tokio::test]
    async fn empty_ids_do_not_build_pending_documents_or_access_database() {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        let client = Client::with_options(options).unwrap();
        let collection = client.database("unit_test").collection("dirty_marks");

        mark_ids(
            &collection,
            &[],
            |_| panic!("empty IDs must not build pending documents"),
            &mut UnusedExecutor,
        )
        .await
        .unwrap();
    }

    #[test]
    fn mark_sets_dirty_and_advances_token() {
        assert_eq!(mark_update(), doc! { "$set": { "dirty": true }, "$inc": { "dirty_token": 1_i64 } });
    }
}
