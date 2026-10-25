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

/// 标记待重算的更新文档。
///
/// # 参数
/// 无。
///
/// # 返回
/// 置 `dirty` 并递增 `dirty_token`。
///
/// # 错误
/// 无。
pub fn mark_update() -> Document {
    doc! { "$set": { "dirty": true }, "$inc": { "dirty_token": 1_i64 } }
}

/// 按主键标记待重算；尚无记录的主键插入占位。
///
/// # 参数
/// * `collection` - 投影集合
/// * `ids` - 主键，允许重复
/// * `pending` - 由主键生成占位文档的其余字段；不含 `_id`、`dirty`、`dirty_token`
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 标记完成时无返回值；空输入不写入。
///
/// # 错误
/// MongoDB 读写失败时返回错误。
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

/// 批量插入占位；并发插入导致主键冲突时逐条按 upsert 补标记。
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

/// 按条件标记待重算。
///
/// # 参数
/// * `collection` - 投影集合
/// * `filter` - 标记条件
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回被标记的记录数。
///
/// # 错误
/// MongoDB 写入失败时返回错误。
pub async fn mark_matching(
    collection: &Collection<Document>,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<u64> {
    Ok(mongo_ops::update_many(collection, filter, mark_update(), executor).await?.modified_count)
}

/// 读取一批待重算记录。
///
/// # 参数
/// * `collection` - 投影集合
/// * `limit` - 本批上限
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回主键与读取时的标记序号。
///
/// # 错误
/// MongoDB 查询失败时返回错误。
pub async fn dirty_batch(
    collection: &Collection<Document>,
    limit: i64,
    executor: &mut dyn Executor,
) -> Result<Vec<DirtyMark>> {
    let options = FindOptions::builder().projection(doc! { "_id": 1, "dirty_token": 1 }).limit(limit).build();
    let marks = collection.clone_with_type::<DirtyMark>();
    mongo_ops::find_many(&marks, doc! { "dirty": true }, options, executor).await
}

/// 统计待重算记录。
///
/// # 参数
/// * `collection` - 投影集合
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回待重算记录数。
///
/// # 错误
/// MongoDB 查询失败时返回错误。
pub async fn dirty_count(collection: &Collection<Document>, executor: &mut dyn Executor) -> Result<u64> {
    mongo_ops::count_documents(collection, doc! { "dirty": true }, executor).await
}

/// 保存计算结果并清除待重算；计算期间又被标记时不保存。
///
/// # 参数
/// * `collection` - 投影集合
/// * `id` - 记录主键
/// * `dirty_token` - 开始计算前读取的标记序号
/// * `fields` - 写回的计算字段；不含主键与标记字段
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 标记序号仍一致并已保存时为真；期间又被标记时为假，留待下一轮。
///
/// # 错误
/// MongoDB 写入失败时返回错误。
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

    struct UnusedExecutor;

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
