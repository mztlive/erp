//! 域 D31 `mall_backfill` 的索引声明：mall_consumption_backfill_job、
//! mall_consumption_backfill_item。
//!
//! 集合名常量取 `MallBackfillExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! §6.17 逐条对照：
//! - `mall_consumption_backfill_item`：「`(job_id, business_fact_key)` 唯一」→
//!   `uk_mall_consumption_backfill_items_key`（★去重唯一索引，不靠应用层查重）；
//!   报告按结果口径统计 → `idx_mall_consumption_backfill_items_result`；
//! - `mall_consumption_backfill_job`：调度按状态拉取待执行作业 →
//!   `idx_mall_consumption_backfill_jobs_status`。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::MallBackfillExt;
use crate::Result;

/// `mall_consumption_backfill_job` 集合名。
pub(crate) const MALL_CONSUMPTION_BACKFILL_JOBS: &str =
    <mongodb::Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_JOBS;
/// `mall_consumption_backfill_item` 集合名。
pub(crate) const MALL_CONSUMPTION_BACKFILL_ITEMS: &str =
    <mongodb::Database as MallBackfillExt>::MALL_CONSUMPTION_BACKFILL_ITEMS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.17「必需约束与索引」；唯一约束一律用唯一索引表达。
/// ★ 回填明细 `(job_id, business_fact_key)` 去重**只靠唯一索引**（P2 计划 §5），
/// 服务层不得做「先查后插」的重复性判断；与实时或其他批次重叠的记录由
/// 唯一索引拒绝后按去重结果落库。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, MALL_CONSUMPTION_BACKFILL_JOBS, backfill_job_indexes()).await?;
    create_indexes(db, MALL_CONSUMPTION_BACKFILL_ITEMS, backfill_item_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `mall_consumption_backfill_job` 的调度查询索引。
fn backfill_job_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_mall_consumption_backfill_jobs_status",
        doc! { "status": 1 },
    )]
}

/// 返回 `mall_consumption_backfill_item` 的去重与报告查询索引。
fn backfill_item_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_consumption_backfill_items_key",
            doc! { "job_id": 1, "business_fact_key": 1 },
        ),
        named_index(
            "idx_mall_consumption_backfill_items_result",
            doc! { "job_id": 1, "result": 1 },
        ),
    ]
}

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{backfill_item_indexes, backfill_job_indexes};

    #[test]
    fn backfill_item_key_index_is_unique() {
        let indexes = backfill_item_indexes();

        let key = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_consumption_backfill_items_key")
            })
            .unwrap();
        assert_eq!(key.keys, doc! { "job_id": 1, "business_fact_key": 1 });
        assert_eq!(key.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "job_id": 1, "result": 1 } }));
    }

    #[test]
    fn backfill_job_status_index_for_scheduler() {
        let indexes = backfill_job_indexes();
        assert!(indexes.iter().any(|index| { index.keys == doc! { "status": 1 } }));
    }
}
