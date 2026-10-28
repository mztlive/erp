//! 批次及逐项草稿使用独立集合，分页调度不依赖浏览器存活。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::indexes::{named_index, unique_index};
use persistence_core::{Executor, Repository, Result, mongo_ops};

use super::{CatalogAdoptionBatch, CatalogAdoptionBatchItem};

const BATCHES: &str = "supplier_catalog_adoption_batches";
const ITEMS: &str = "supplier_catalog_adoption_batch_items";

/// 作业批次仓储；更新统一使用 Repository 的实体版本 CAS。
pub trait BatchAdoptionExt {
    /// 取得批次仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库的批次仓储。
    /// # 错误
    /// 不返回错误。
    fn catalog_adoption_batches(&self) -> Repository<'_, CatalogAdoptionBatch>;

    /// 取得逐项草稿及冻结执行记录仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库的批次行仓储。
    /// # 错误
    /// 不返回错误。
    fn catalog_adoption_batch_items(&self) -> Repository<'_, CatalogAdoptionBatchItem>;
}

impl BatchAdoptionExt for Database {
    fn catalog_adoption_batches(&self) -> Repository<'_, CatalogAdoptionBatch> {
        Repository::new(self, BATCHES)
    }

    fn catalog_adoption_batch_items(&self) -> Repository<'_, CatalogAdoptionBatchItem> {
        Repository::new(self, ITEMS)
    }
}

/// 按稳定游标读取同一批次冻结成员，不截断或混入其他批次。
/// # 参数
/// `batch` 为已授权批次，`after` 为上一页最后行主键，`limit` 为页大小。
/// # 返回
/// 按主键升序的至多一千行；调用方继续读取直到空页。
/// # 错误
/// 数据库读取失败时返回错误。
pub async fn batch_items(
    db: &Database,
    batch: &str,
    after: &str,
    limit: u32,
    executor: &mut dyn Executor,
) -> Result<Vec<CatalogAdoptionBatchItem>> {
    mongo_ops::find_many(
        &db.collection::<CatalogAdoptionBatchItem>(ITEMS),
        doc! {"batch_id":batch,"id":{"$gt":after},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).limit(i64::from(limit.clamp(1, 1000))).build(),
        executor,
    )
    .await
}

/// 按当前已授权连接读取可重开的批次摘要。
/// # 参数
/// `connection` 必须由流程重新验证权限和数据范围，`before` 为稳定主键游标。
/// # 返回
/// 最近二十个批次，不包含其他连接。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn recent_batches(
    db: &Database,
    connection: &str,
    before: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<CatalogAdoptionBatch>> {
    let mut filter = doc! {"connection_id":connection,"selection_complete":true,"deleted_at":0_i64};
    if let Some(before) = before {
        filter.insert("id", doc! {"$lt":before});
    }
    mongo_ops::find_many(
        &db.collection::<CatalogAdoptionBatch>(BATCHES),
        filter,
        FindOptions::builder().sort(doc! {"id":-1}).limit(20).build(),
        executor,
    )
    .await
}

/// 有界读取已确认且需后台检查的批次。
/// # 参数
/// `after` 为上轮最后处理批次的主键，空串从头读取。
/// # 返回
/// 稳定顺序的最多二十个已完成成员冻结且仍有执行任务的批次。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn runnable_batches(
    db: &Database,
    after: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<CatalogAdoptionBatch>> {
    mongo_ops::find_many(
        &db.collection::<CatalogAdoptionBatch>(BATCHES),
        doc! {"submitted":true,"selection_complete":true,"execution_pending":true,"id":{"$gt":after},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).limit(20).build(),
        executor,
    )
    .await
}

/// 取得本批已明确确认且尚未领取的行。
/// # 参数
/// `batch` 为调度流程已读取的批次；读取不代表领取成功。
/// # 返回
/// 最多五十项；执行前须调用实体 begin 并 CAS 保存。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn due_batch_items(
    db: &Database,
    batch: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<CatalogAdoptionBatchItem>> {
    mongo_ops::find_many(
        &db.collection::<CatalogAdoptionBatchItem>(ITEMS),
        doc! {"batch_id":batch,"status":"READY","operation_id":{"$type":"string","$ne":""},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).limit(50).build(),
        executor,
    )
    .await
}

/// 判断本批是否存在正在执行或已经明确排队的行。
/// # 参数
/// `batch` 为已授权批次，`executor` 沿用草稿操作的短事务。
/// # 返回
/// 命中处理中或带有效原操作号的就绪行时返回真；未知结果不计入。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn has_inflight_batch_items(
    db: &Database,
    batch: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    db.catalog_adoption_batch_items()
        .exists(
            doc! {"batch_id":batch,"deleted_at":0_i64,"$or":[{"status":"PROCESSING"},{"status":"READY","operation_id":{"$type":"string","$ne":""}}]},
            executor,
        )
        .await
}

/// 读取需要查证原结果的行，禁止将中断操作重新当作待执行行。
/// # 参数
/// `stale_before` 为超过允许处理时间的服务器时间，`batch` 为已授权批次，`after` 为上页最后行主键。
/// # 返回
/// 最多五十项未知结果或处理超时的原操作。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn uncertain_batch_items(
    db: &Database,
    batch: &str,
    after: &str,
    stale_before: i64,
    executor: &mut dyn Executor,
) -> Result<Vec<CatalogAdoptionBatchItem>> {
    mongo_ops::find_many(
        &db.collection::<CatalogAdoptionBatchItem>(ITEMS),
        doc! {"batch_id":batch,"id":{"$gt":after},"deleted_at":0_i64,"$or":[{"status":"UNKNOWN"},{"status":"PROCESSING","processing_started_at":{"$lte":stale_before}}]},
        FindOptions::builder().sort(doc! {"id":1}).limit(50).build(),
        executor,
    )
    .await
}

/// 登记独立批次、来源成员身份及后台任务扫描索引。
/// # 参数
/// `db` 为应用数据库。
/// # 返回
/// 全部增量索引创建成功。
/// # 错误
/// 存量唯一冲突或数据库失败时返回错误。
pub async fn ensure_batch_indexes(db: &Database) -> Result<()> {
    db.collection::<Document>(BATCHES)
        .create_indexes([
            unique_index("uk_catalog_adoption_batch_id", doc! {"id":1}),
            unique_index("uk_catalog_adoption_batch_no", doc! {"batch_no":1}),
            named_index(
                "idx_catalog_adoption_batch_connection",
                doc! {"connection_id":1,"selection_complete":1,"id":-1},
            ),
            named_index("idx_catalog_adoption_batch_actor", doc! {"connection_id":1,"actor_id":1,"id":-1}),
            named_index(
                "idx_catalog_adoption_batch_runnable",
                doc! {"execution_pending":1,"selection_complete":1,"submitted":1,"id":1},
            ),
        ])
        .await?;
    db.collection::<Document>(ITEMS)
        .create_indexes([
            unique_index("uk_catalog_adoption_batch_item_id", doc! {"id":1}),
            unique_index("uk_catalog_adoption_batch_source", doc! {"batch_id":1,"source.id":1}),
            named_index("idx_catalog_adoption_batch_item_page", doc! {"batch_id":1,"id":1}),
            named_index("idx_catalog_adoption_batch_item_due", doc! {"batch_id":1,"status":1,"id":1}),
            named_index(
                "idx_catalog_adoption_batch_item_stale",
                doc! {"batch_id":1,"status":1,"processing_started_at":1},
            ),
        ])
        .await?;
    Ok(())
}

/// 在已授权批次中读取唯一来源 SKU 的草稿与冻结载荷。
/// # 参数
/// `batch` 为流程已授权的批次，`source_item` 为冻结来源身份，`executor` 为当前事务。
/// # 返回
/// 返回该批次唯一来源行；不存在时为空。
/// # 错误
/// 数据库读取失败时返回错误。
pub async fn batch_item_by_source(
    db: &Database,
    batch: &str,
    source_item: &str,
    executor: &mut dyn Executor,
) -> Result<Option<CatalogAdoptionBatchItem>> {
    db.catalog_adoption_batch_items()
        .find_one(doc! {"batch_id":batch,"source.id":source_item}, executor)
        .await
}
