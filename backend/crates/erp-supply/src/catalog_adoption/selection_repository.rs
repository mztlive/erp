//! 固定成员身份清单，每份文档最多二百个来源身份。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::indexes::{named_index, unique_index, unique_partial_index};
use persistence_core::{Executor, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use super::{BatchAdoptionExt, CatalogAdoptionBatch, CatalogAdoptionBatchItem};

const SELECTIONS: &str = "supplier_catalog_adoption_selections";
pub const SELECTION_PAGE_SIZE: usize = 200;

/// 受理时冻结的成员身份；不保存可变化的供应商报价。
#[derive(Debug, Serialize, Deserialize)]
pub struct SelectionPage {
    pub batch_id: String,
    pub page: u32,
    pub item_ids: Vec<String>,
}

/// 查找原操作已登记的批次，用于受理超时后的重试恢复。
/// # 参数
/// `request_key` 为由连接、账号及原操作号组成的稳定身份。
/// # 返回
/// 原批次，未登记时为空；调用方仍须验证当前权限和载荷摘要。
/// # 错误
/// 数据库读取失败。
pub async fn selection_by_request(
    db: &Database,
    request_key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<CatalogAdoptionBatch>> {
    db.catalog_adoption_batches().find_one(doc! {"preparation.request_key":request_key}, executor).await
}

/// 批量保存固定成员分块，必须与批次登记共用事务。
/// # 参数
/// `pages` 为同一批次的完整成员清单，`executor` 为受理事务。
/// # 返回
/// 全部分块写入成功。
/// # 错误
/// 数据库写入或唯一身份冲突时失败。
pub async fn insert_selection_pages(
    db: &Database,
    pages: &[SelectionPage],
    executor: &mut dyn Executor,
) -> Result<()> {
    mongo_ops::insert_many(&db.collection::<SelectionPage>(SELECTIONS), pages, executor).await
}

/// 读取下一块已冻结成员，禁止重新运行原筛选条件。
/// # 参数
/// `batch` 为已授权批次，`page` 为原子进度对应分块。
/// # 返回
/// 对应固定成员，不存在时为空。
/// # 错误
/// 数据库读取失败。
pub async fn selection_page(
    db: &Database,
    batch: &str,
    page: u32,
    executor: &mut dyn Executor,
) -> Result<Option<SelectionPage>> {
    mongo_ops::find_one(
        &db.collection::<SelectionPage>(SELECTIONS),
        doc! {"batch_id":batch,"page":i64::from(page)},
        executor,
    )
    .await
}

/// 一次写入一块草稿，调用方须同时保存准备进度。
/// # 参数
/// `rows` 为当前分块草稿，`executor` 为同一短事务。
/// # 返回
/// 整块写入成功。
/// # 错误
/// 数据库错误、重复来源身份或重复行身份时拒绝。
pub async fn insert_prepared_items(
    db: &Database,
    rows: &[CatalogAdoptionBatchItem],
    executor: &mut dyn Executor,
) -> Result<()> {
    mongo_ops::insert_many(
        &db.collection::<CatalogAdoptionBatchItem>("supplier_catalog_adoption_batch_items"),
        rows,
        executor,
    )
    .await
}

/// 登记成员分块唯一索引与准备任务扫描索引。
/// # 参数
/// `db` 为当前数据库。
/// # 返回
/// 索引全部建立。
/// # 错误
/// 数据库失败或存量唯一冲突时失败。
pub async fn ensure_preparation_indexes(db: &Database) -> Result<()> {
    db.collection::<Document>(SELECTIONS)
        .create_index(unique_index("uk_catalog_adoption_selection_page", doc! {"batch_id":1,"page":1}))
        .await?;
    db.collection::<Document>("supplier_catalog_adoption_batches")
        .create_index(named_index("idx_catalog_adoption_preparation", doc! {"preparation.status":1,"id":1}))
        .await?;
    db.collection::<Document>("supplier_catalog_adoption_batches")
        .create_indexes([
            unique_partial_index(
                "uk_catalog_adoption_preparation_request",
                doc! {"preparation.request_key":1},
                doc! {"preparation.request_key":{"$type":"string"}},
            ),
            named_index("idx_catalog_adoption_batch_recent", doc! {"connection_id":1,"id":-1}),
        ])
        .await?;
    Ok(())
}
