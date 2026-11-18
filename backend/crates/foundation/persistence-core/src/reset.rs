//! 显式数据库重置机制：保留指定物理身份，清空其余文档，保留集合与索引。

use std::collections::HashMap;

use async_trait::async_trait;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};

use crate::{Error, Executor, Result, mongo_ops};

/// 按集合登记必须原样保留的 MongoDB 文档身份。
#[derive(Debug, Default)]
pub struct ResetRetention(HashMap<String, Vec<Bson>>);

impl ResetRetention {
    /// 事务内清库前，按领域的精确条件查出必须保留的记录，登记原始 BSON `_id`，不修改数据库。此入口不单独检查事务；
    /// 记录或物理主键缺失时报元数据错误。
    pub async fn require(
        &mut self,
        db: &Database,
        collection: &str,
        filter: Document,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut document = mongo_ops::find_one(&db.collection::<Document>(collection), filter, executor)
            .await?
            .ok_or(Error::EntityMetadataOutOfRange("reset retention document missing"))?;

        let id = document
            .remove("_id")
            .ok_or(Error::EntityMetadataOutOfRange("reset retention document has no _id"))?;
        self.0.entry(collection.into()).or_default().push(id);
        Ok(())
    }
}

/// 目标数据库当前的普通集合清单，不包含 MongoDB 系统集合。
pub struct DatabaseReset {
    db: Database,
    collections: Vec<String>,
}

impl DatabaseReset {
    /// 重置前在事务外从配置的数据库读取集合清单，排除系统集合但涵盖未登记的历史集合；
    /// 不能用客户端传来的集合白名单代替。
    pub async fn prepare(db: Database) -> Result<Self> {
        let mut collections = db.list_collection_names().await?;
        collections.retain(|name| !name.starts_with("system."));
        collections.sort();
        Ok(Self { db, collections })
    }

    /// 须在受管快照事务中执行，按领域核准的物理主键保留文档，删除其余内容但不动集合或索引。返回删除总数；
    /// 保留集合不存在、计数溢出、仍有残留或数据库失败时交外层回滚。
    pub async fn execute(self, retention: ResetRetention, executor: &mut dyn Executor) -> Result<u64> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::UnsupportedDeployment("database reset requires a transaction"));
        }
        if retention.0.keys().any(|name| !self.collections.contains(name)) {
            return Err(Error::EntityMetadataOutOfRange("reset retention collection missing"));
        }

        let mut store = MongoResetStore { db: &self.db, executor };
        execute(&mut store, &self.collections, &retention).await
    }
}

/// 重置编排使用的持久化操作，测试替身只替换 I/O。
#[async_trait]
trait ResetStore: Send {
    // 按计划删去保留主键以外的文档，供编排累加删除条数；删除失败交外层事务处理。
    async fn erase(&mut self, collection: &str, kept: &[Bson]) -> Result<u64>;
    // 清理后查保留主键以外还有无文档，防止不完整的重置被当作成功提交。
    async fn has_residue(&mut self, collection: &str, kept: &[Bson]) -> Result<bool>;
}

struct MongoResetStore<'a> {
    db: &'a Database,
    executor: &'a mut dyn Executor,
}

#[async_trait]
impl ResetStore for MongoResetStore<'_> {
    async fn erase(&mut self, collection: &str, kept: &[Bson]) -> Result<u64> {
        Ok(mongo_ops::delete_many(
            &self.db.collection::<Document>(collection),
            deletion_filter(kept),
            self.executor,
        )
        .await?
        .deleted_count)
    }

    async fn has_residue(&mut self, collection: &str, kept: &[Bson]) -> Result<bool> {
        mongo_ops::exists(&self.db.collection::<Document>(collection), deletion_filter(kept), self.executor)
            .await
    }
}

// 用 BSON 物理主键排除保留记录，不依赖业务 ID；集合没有保留项时用空条件全部清除。
fn deletion_filter(kept: &[Bson]) -> Document {
    if kept.is_empty() {
        doc! {}
    } else {
        doc! { "_id": { "$nin": kept } }
    }
}

// 先按计划清理各集合并累加条数，再逐一检查残留；任一步失败、溢出或残留立即向外层事务报错。
async fn execute(store: &mut impl ResetStore, collections: &[String], kept: &ResetRetention) -> Result<u64> {
    let mut removed = 0_u64;
    for collection in collections {
        let ids = kept.0.get(collection).map(Vec::as_slice).unwrap_or_default();
        removed = removed
            .checked_add(store.erase(collection, ids).await?)
            .ok_or(Error::EntityMetadataOutOfRange("reset deleted count overflow"))?;
    }

    for collection in collections {
        let ids = kept.0.get(collection).map(Vec::as_slice).unwrap_or_default();
        if store.has_residue(collection, ids).await? {
            return Err(Error::EntityMetadataOutOfRange("database reset left residual documents"));
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    #[derive(Default)]
    struct MemoryStore {
        rows: HashMap<String, Vec<Bson>>,
        calls: Vec<String>,
        fail: Option<String>,
        residue: bool,
    }

    #[async_trait]
    impl ResetStore for MemoryStore {
        async fn erase(&mut self, collection: &str, kept: &[Bson]) -> Result<u64> {
            self.calls.push(collection.into());
            if self.fail.as_deref() == Some(collection) {
                return Err(Error::OptimisticLockingError);
            }
            let rows = self.rows.entry(collection.into()).or_default();
            let before = rows.len();
            rows.retain(|id| kept.contains(id));
            Ok((before - rows.len()) as u64)
        }
        async fn has_residue(&mut self, collection: &str, kept: &[Bson]) -> Result<bool> {
            Ok(self.residue || self.rows[collection].iter().any(|id| !kept.contains(id)))
        }
    }

    #[test]
    fn reset_preserves_exact_ids_and_clears_unregistered_collections_and_replays() {
        let mut store = MemoryStore::default();
        store.rows.insert("identities".into(), vec![Bson::from("keep"), Bson::from("remove")]);
        store.rows.insert("historical_unregistered".into(), vec![Bson::from(7)]);
        store.rows.insert("empty".into(), vec![]);
        let collections = store.rows.keys().cloned().collect::<Vec<_>>();
        let kept = ResetRetention(HashMap::from([("identities".into(), vec![Bson::from("keep")])]));

        assert_eq!(execute(&mut store, &collections, &kept).now_or_never().unwrap().unwrap(), 2);
        assert_eq!(store.rows["identities"], vec![Bson::from("keep")]);
        assert!(store.rows["historical_unregistered"].is_empty());

        assert_eq!(execute(&mut store, &collections, &kept).now_or_never().unwrap().unwrap(), 0);
    }

    #[test]
    fn failure_stops_execution_and_residue_is_rejected() {
        let mut store = MemoryStore { fail: Some("first".into()), ..Default::default() };
        let collections = vec!["first".into(), "second".into()];
        assert!(
            execute(&mut store, &collections, &ResetRetention::default()).now_or_never().unwrap().is_err()
        );
        assert_eq!(store.calls, vec!["first"]);

        store.fail = None;
        store.residue = true;
        assert!(
            execute(&mut store, &collections, &ResetRetention::default()).now_or_never().unwrap().is_err()
        );
    }
}
