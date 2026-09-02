//! 商城销售单快照落盘范围与单调写入（INT-R16）。
//!
//! 一次批量返回 exact 事实键与各来源单 latest 最小事实（固定两次有界读取），
//! 并提供调用方 executor 下的批量插入与单调水位 CAS。不开事务；分类与作业
//! 进度仍由 Entity / Service 负责。

use std::collections::HashSet;

use super::super::extensions::MallSyncExt;
use super::MallSyncRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};
use entities::mall_sync::{MallSalesOrderSnapshot, SnapshotFactIdentity, SnapshotOrderIdentity};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

/// `mall_sales_order_snapshot` 集合名（单一来源：`MallSyncExt`）。
const MALL_SALES_ORDER_SNAPSHOTS: &str = <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS;
/// `mall_sales_order_snapshot_watermarks` 集合名（单一来源：`MallSyncExt`）。
const MALL_SALES_ORDER_SNAPSHOT_WATERMARKS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOT_WATERMARKS;

/// 快照落盘所需的持久化最小事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotIngestScope {
    /// 库内已存在的精确事实键（请求中缺失的键不出现）。
    pub exact_keys: Vec<SnapshotFactIdentity>,
    /// 各来源单当前最新快照最小事实（无历史的来源单不出现）。
    pub latest: Vec<SnapshotFactIdentity>,
}

impl<'a> MallSyncRepository<'a> {
    /// 批量读取 exact 事实键与各来源单 latest 最小事实（INT-R16）。
    ///
    /// 固定两次数据库访问：按去重后的事实键 `$or` 取 exact 命中；再按去重后的
    /// 来源单身份聚合最新 `source_updated_at`。空输入不访问数据库。全部使用
    /// 调用方执行器，事务内调用看到同一事务未提交写入。
    ///
    /// # 参数
    /// * `candidates` - 本页候选项（可含重复）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 exact 键与 latest 最小事实；缺项由 Entity 解释。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 不返回 services DTO、HTTP View 或授权结论；不裁决 Duplicate/Stale/Accept。
    pub async fn snapshot_ingest_scope(
        &self,
        candidates: &[SnapshotFactIdentity],
        executor: &mut dyn Executor,
    ) -> Result<SnapshotIngestScope> {
        let exact_keys = unique_fact_identities(candidates);
        if exact_keys.is_empty() {
            return Ok(SnapshotIngestScope {
                exact_keys: Vec::new(),
                latest: Vec::new(),
            });
        }
        let exact_keys = self.load_exact_keys(&exact_keys, executor).await?;
        let orders = unique_order_identities(candidates);
        let latest = self.load_latest_facts(&orders, executor).await?;
        Ok(SnapshotIngestScope { exact_keys, latest })
    }

    /// 在调用方 executor 下批量插入已分类为 Accept 的快照。
    ///
    /// 空集合不访问数据库。唯一键冲突透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `snapshots` - 待插入快照
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 全部插入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    ///
    /// # 约束
    /// 本方法不开启或提交事务；失败时是否部分可见由调用方事务决定。
    pub async fn create_snapshots(
        &self,
        snapshots: &[MallSalesOrderSnapshot],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self
                .db
                .collection::<MallSalesOrderSnapshot>(MALL_SALES_ORDER_SNAPSHOTS),
            snapshots.to_vec(),
            executor,
        )
        .await
    }

    /// 单调推进来源单快照水位（INT-R16 并发守卫）。
    ///
    /// 每个来源单一次 `$max` upsert：仅当库内水位严格小于候选项时推进。
    /// 等时或已有更新版本视为未夺得（`false`）。并发首次插入撞唯一键时
    /// **不在本会话内捕获 E11000**，由调用方中止事务、换新会话后重分类。
    ///
    /// # 参数
    /// * `claims` - 拟接受快照的事实身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回与 `claims` 等长的夺得标记；`true` 表示水位已推进到候选项时间。
    ///
    /// # 错误
    /// 唯一键冲突或 MongoDB 写入失败时返回错误。
    ///
    /// # 约束
    /// 水位集合唯一索引 `(source_system_id, external_order_key)` 是并发权威；
    /// 等时唯一索引不能替代本单调约束。禁止在失败会话上继续读写。
    pub async fn claim_snapshot_watermarks(
        &self,
        claims: &[SnapshotFactIdentity],
        executor: &mut dyn Executor,
    ) -> Result<Vec<bool>> {
        let mut won = Vec::with_capacity(claims.len());
        for claim in claims {
            won.push(self.claim_one_watermark(claim, executor).await?);
        }
        Ok(won)
    }

    /// 按去重事实键批量装载已存在的精确键。
    ///
    /// # 参数
    /// * `keys` - 已去重事实键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回库内命中的精确键；缺失键不生成条目。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_exact_keys(
        &self,
        keys: &[SnapshotFactIdentity],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SnapshotFactIdentity>> {
        let Some(filter) = exact_keys_filter(keys) else {
            return Ok(Vec::new());
        };
        let options = FindOptions::builder()
            .projection(snapshot_fact_projection())
            .build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<SnapshotFactIdentity>(MALL_SALES_ORDER_SNAPSHOTS),
            filter,
            options,
            executor,
        )
        .await
    }

    /// 按来源单身份聚合最新 `source_updated_at`。
    ///
    /// # 参数
    /// * `orders` - 已去重来源单身份
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回各来源单 latest 最小事实；无历史的来源单不出现。
    ///
    /// # 错误
    /// MongoDB 聚合失败时返回错误。
    async fn load_latest_facts(
        &self,
        orders: &[SnapshotOrderIdentity],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SnapshotFactIdentity>> {
        let Some(pipeline) = latest_facts_pipeline(orders) else {
            return Ok(Vec::new());
        };
        aggregate_fact_identities(
            &self
                .db
                .collection::<SnapshotFactIdentity>(MALL_SALES_ORDER_SNAPSHOTS),
            pipeline,
            executor,
        )
        .await
    }

    /// 为单个来源单执行一次 `$max` upsert。
    ///
    /// # 参数
    /// * `claim` - 拟接受的事实身份
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 插入新水位或严格推进时返回 `true`；等时/更旧返回 `false`。
    ///
    /// # 错误
    /// 并发首次插入撞唯一键或 MongoDB 写入失败时原样返回，不在本会话重试。
    async fn claim_one_watermark(
        &self,
        claim: &SnapshotFactIdentity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let collection = self
            .db
            .collection::<Document>(MALL_SALES_ORDER_SNAPSHOT_WATERMARKS);
        let result = mongo_ops::update_one(
            &collection,
            watermark_identity_filter(claim),
            watermark_claim_update(claim),
            true,
            executor,
        )
        .await?;
        Ok(watermark_claim_won(
            result.upserted_id.is_some(),
            result.modified_count,
        ))
    }
}

/// 按首次出现顺序去重事实键。
///
/// # 参数
/// * `keys` - 原始事实键（可含重复）
///
/// # 返回
/// 返回去重后的事实键。
fn unique_fact_identities(keys: &[SnapshotFactIdentity]) -> Vec<SnapshotFactIdentity> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for key in keys {
        if seen.insert(key.clone()) {
            unique.push(key.clone());
        }
    }
    unique
}

/// 按首次出现顺序去重来源单身份。
///
/// # 参数
/// * `keys` - 原始事实键（可含重复来源单）
///
/// # 返回
/// 返回去重后的来源单身份。
fn unique_order_identities(keys: &[SnapshotFactIdentity]) -> Vec<SnapshotOrderIdentity> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for key in keys {
        let order = key.order_identity();
        if seen.insert(order.clone()) {
            unique.push(order);
        }
    }
    unique
}

/// 构造 exact 事实键查询；空输入返回 `None` 以免发送非法 `$or`。
///
/// # 参数
/// * `keys` - 已去重事实键
///
/// # 返回
/// 返回带软删除过滤的 `$or` 文档。
fn exact_keys_filter(keys: &[SnapshotFactIdentity]) -> Option<Document> {
    if keys.is_empty() {
        return None;
    }
    let clauses: Vec<Document> = keys
        .iter()
        .map(|key| {
            doc! {
                "source_system_id": key.source_system_id.to_string(),
                "external_order_key": key.external_order_key.to_bson_binary(),
                "source_updated_at": key.source_updated_at.unix_secs(),
            }
        })
        .collect();
    Some(doc! {
        "$or": clauses,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 构造来源单 latest 聚合管道；空输入返回 `None`。
///
/// # 参数
/// * `orders` - 已去重来源单身份
///
/// # 返回
/// 返回 `$match` + `$sort` + `$group` 管道。
fn latest_facts_pipeline(orders: &[SnapshotOrderIdentity]) -> Option<Vec<Document>> {
    let filter = latest_orders_filter(orders)?;
    Some(vec![
        doc! { "$match": filter },
        doc! { "$sort": { "source_updated_at": -1, "id": 1 } },
        doc! {
            "$group": {
                "_id": {
                    "source_system_id": "$source_system_id",
                    "external_order_key": "$external_order_key",
                },
                "source_updated_at": { "$first": "$source_updated_at" },
            }
        },
        doc! {
            "$project": {
                "_id": 0,
                "source_system_id": "$_id.source_system_id",
                "external_order_key": "$_id.external_order_key",
                "source_updated_at": 1,
            }
        },
    ])
}

/// 构造来源单 `$or` 过滤；空输入返回 `None`。
///
/// # 参数
/// * `orders` - 已去重来源单身份
///
/// # 返回
/// 返回带软删除过滤的文档。
fn latest_orders_filter(orders: &[SnapshotOrderIdentity]) -> Option<Document> {
    if orders.is_empty() {
        return None;
    }
    let clauses: Vec<Document> = orders
        .iter()
        .map(|order| {
            doc! {
                "source_system_id": order.source_system_id.to_string(),
                "external_order_key": order.external_order_key.to_bson_binary(),
            }
        })
        .collect();
    Some(doc! {
        "$or": clauses,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 快照事实最小投影。
///
/// # 返回
/// 返回身份三元组投影。
fn snapshot_fact_projection() -> Document {
    doc! {
        "_id": 0,
        "source_system_id": 1,
        "external_order_key": 1,
        "source_updated_at": 1,
    }
}

/// 构造水位身份等值过滤。
///
/// # 参数
/// * `claim` - 拟接受的事实身份
///
/// # 返回
/// 返回来源单等值文档。
fn watermark_identity_filter(claim: &SnapshotFactIdentity) -> Document {
    doc! {
        "source_system_id": claim.source_system_id.to_string(),
        "external_order_key": claim.external_order_key.to_bson_binary(),
    }
}

/// 构造单次 `$max` upsert 更新。
///
/// # 参数
/// * `claim` - 拟接受的事实身份
///
/// # 返回
/// 返回 `$max` + `$setOnInsert` 文档。
fn watermark_claim_update(claim: &SnapshotFactIdentity) -> Document {
    doc! {
        "$max": { "source_updated_at": claim.source_updated_at.unix_secs() },
        "$setOnInsert": {
            "source_system_id": claim.source_system_id.to_string(),
            "external_order_key": claim.external_order_key.to_bson_binary(),
        },
    }
}

/// 判断 `$max` upsert 是否夺得候选项时间。
///
/// # 参数
/// * `upserted` - 是否插入新水位
/// * `modified_count` - 被修改文档数
///
/// # 返回
/// 新插入或严格推进时返回 `true`。
fn watermark_claim_won(upserted: bool, modified_count: u64) -> bool {
    upserted || modified_count == 1
}

/// 执行事实身份聚合管道。
///
/// # 参数
/// * `collection` - 快照集合
/// * `pipeline` - 聚合管道
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回全部聚合行。
///
/// # 错误
/// 聚合或反序列化失败时返回错误。
async fn aggregate_fact_identities(
    collection: &mongodb::Collection<SnapshotFactIdentity>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<SnapshotFactIdentity>> {
    match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<SnapshotFactIdentity>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await
        }
        None => {
            collection
                .aggregate(pipeline)
                .with_type::<SnapshotFactIdentity>()
                .await?
                .try_collect::<Vec<_>>()
                .await
        }
    }
    .map_err(crate::Error::from)
}

#[cfg(test)]
mod tests {
    use super::{
        exact_keys_filter, latest_facts_pipeline, latest_orders_filter, unique_fact_identities,
        unique_order_identities, watermark_claim_update, watermark_claim_won, watermark_identity_filter,
    };
    use entities::common::time::Instant;
    use entities::ids::SourceSystemId;
    use entities::mall_sync::{ExternalOrderKey, SnapshotFactIdentity};
    use mongodb::bson::doc;

    fn fact(order: &str, secs: i64) -> SnapshotFactIdentity {
        SnapshotFactIdentity::new(
            SourceSystemId::new("sys-mall"),
            ExternalOrderKey::from_trimmed(order),
            Instant::from_unix_secs(secs),
        )
    }

    #[test]
    fn unique_fact_identities_dedups_and_handles_empty() {
        assert!(unique_fact_identities(&[]).is_empty());
        let keys = unique_fact_identities(&[fact("SO-1", 10), fact("SO-2", 10), fact("SO-1", 10)]);
        assert_eq!(keys, vec![fact("SO-1", 10), fact("SO-2", 10)]);
    }

    #[test]
    fn unique_order_identities_dedups_same_order_different_times() {
        assert!(unique_order_identities(&[]).is_empty());
        let orders = unique_order_identities(&[fact("SO-1", 10), fact("SO-1", 20), fact("SO-2", 5)]);
        assert_eq!(orders.len(), 2);
        assert_eq!(
            orders[0].external_order_key,
            ExternalOrderKey::from_trimmed("SO-1")
        );
        assert_eq!(
            orders[1].external_order_key,
            ExternalOrderKey::from_trimmed("SO-2")
        );
    }

    #[test]
    fn exact_keys_filter_is_none_when_empty_and_includes_deleted_guard() {
        assert!(exact_keys_filter(&[]).is_none());
        let filter = exact_keys_filter(&[fact("SO-1", 10)]).unwrap();
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let clauses = filter.get_array("$or").unwrap();
        assert_eq!(clauses.len(), 1);
    }

    #[test]
    fn latest_orders_filter_and_pipeline_are_none_when_empty() {
        assert!(latest_orders_filter(&[]).is_none());
        assert!(latest_facts_pipeline(&[]).is_none());
        let pipeline = latest_facts_pipeline(&[fact("SO-1", 10).order_identity()]).unwrap();
        assert_eq!(pipeline.len(), 4);
        assert!(pipeline[0].get_document("$match").unwrap().contains_key("$or"));
        assert_eq!(
            pipeline[1].get_document("$sort").unwrap(),
            &doc! { "source_updated_at": -1, "id": 1 }
        );
    }

    #[test]
    fn watermark_claim_update_is_single_max_upsert() {
        let claim = fact("SO-1", 20);
        let filter = watermark_identity_filter(&claim);
        assert_eq!(filter.get_str("source_system_id").unwrap(), "sys-mall");
        assert!(!filter.contains_key("source_updated_at"));
        let update = watermark_claim_update(&claim);
        let max = update.get_document("$max").unwrap();
        assert_eq!(max.get_i64("source_updated_at").unwrap(), 20);
        assert!(update.contains_key("$setOnInsert"));
    }

    #[test]
    fn watermark_claim_won_only_on_insert_or_strict_advance() {
        assert!(watermark_claim_won(true, 0));
        assert!(watermark_claim_won(false, 1));
        assert!(!watermark_claim_won(false, 0));
    }
}
