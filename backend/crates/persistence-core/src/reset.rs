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
    /// 精确读取必须保留的文档，并登记其物理主键。
    /// # 参数
    /// `db` 为数据库，`collection` 与 `filter` 由拥有领域提供，`executor` 为当前事务。
    /// # 返回
    /// 文档存在时登记原始 `_id`，不修改其内容。
    /// # 错误
    /// 必需文档缺失、没有物理主键或读取失败时拒绝重置。
    pub async fn require(
        &mut self,
        db: &Database,
        collection: &str,
        filter: Document,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let document = mongo_ops::find_one(&db.collection::<Document>(collection), filter, executor)
            .await?
            .ok_or(Error::EntityMetadataOutOfRange("reset retention document missing"))?;
        let id = document
            .get("_id")
            .ok_or(Error::EntityMetadataOutOfRange("reset retention document has no _id"))?;
        self.0.entry(collection.into()).or_default().push(id.clone());
        Ok(())
    }
}

/// 目标数据库当前的普通集合清单，不包含 MongoDB 系统集合。
pub struct DatabaseReset {
    db: Database,
    collections: Vec<String>,
}

impl DatabaseReset {
    /// 在事务外读取集合元数据；不得由客户端提交集合白名单。
    /// # 参数
    /// `db` 为应用配置指定的目标数据库。
    /// # 返回
    /// 返回当前数据库的重置计划，包含未登记的历史业务集合。
    /// # 错误
    /// MongoDB 元数据读取失败时返回错误。
    pub async fn prepare(db: Database) -> Result<Self> {
        let mut collections = db.list_collection_names().await?;
        collections.retain(|name| !name.starts_with("system."));
        collections.sort();
        Ok(Self { db, collections })
    }

    /// 在调用方事务中清除保留清单之外的全部文档。
    /// # 参数
    /// `retention` 为领域核准的物理身份，`executor` 必须承载事务。
    /// # 返回
    /// 实际删除的文档总数；集合与索引保持不变。
    /// # 错误
    /// 无事务、删除失败、保留集合缺失或检测到残留时返回错误，由调用方回滚。
    pub async fn execute(self, retention: ResetRetention, executor: &mut dyn Executor) -> Result<u64> {
        if executor.session().is_none() {
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
    async fn erase(&mut self, collection: &str, kept: &[Bson]) -> Result<u64>;
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

/// 精确排除物理主键，未登记保留身份的集合全部清空。
fn deletion_filter(kept: &[Bson]) -> Document {
    if kept.is_empty() {
        doc! {}
    } else {
        doc! { "_id": { "$nin": kept } }
    }
}

/// 顺序执行完整计划并核对残留，任何失败立即交给外层事务处理。
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
