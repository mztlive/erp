//! 后台草稿请求与有界批量 CAS 更新。

use std::collections::HashSet;

use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::{Document, doc, serialize_to_document};
use persistence_core::indexes::{named_index, unique_index};
use persistence_core::{Error, Executor, Repository, Result, mongo_ops};

use super::{BatchWorkRequest, CatalogAdoptionBatchItem};

const WORK: &str = "supplier_catalog_adoption_work_requests";

/// 取得持久化草稿请求仓储。
/// # 参数
/// `db` 为当前数据库。
/// # 返回
/// 仅保存请求，进度由批次头负责。
/// # 错误
/// 不返回错误。
pub fn work_requests(db: &Database) -> Repository<'_, BatchWorkRequest> {
    Repository::new(db, WORK)
}

/// 使用单次更新管道保存有限多行，保留 Mongo 内部身份并核验每行版本。
/// # 参数
/// `rows` 为最多二百行、同一批次的完整新草稿；执行器必须处于短事务。
/// # 返回
/// 所有匹配行已更新；调用方同时提交任务进度。
/// # 错误
/// 行身份重复、版本冲突或数据库失败时返回错误，调用方必须回滚事务。
pub async fn update_draft_chunk(
    db: &Database,
    rows: &[CatalogAdoptionBatchItem],
    executor: &mut dyn Executor,
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::EntityMetadataOutOfRange("批量草稿更新必须使用事务"));
    }

    let (filter, update) = draft_chunk_update(rows, Instant::now().unix_secs())?;
    let collection = db.collection::<Document>("supplier_catalog_adoption_batch_items");
    let result = mongo_ops::update_many_pipeline(&collection, filter, update, executor).await?;
    if usize::try_from(result.matched_count).ok() != Some(rows.len()) {
        return Err(Error::OptimisticLockingError);
    }
    Ok(())
}

/// 生成版本受控的文档替换管道，所有业务文本以 literal 写入。
fn draft_chunk_update(rows: &[CatalogAdoptionBatchItem], now: i64) -> Result<(Document, Vec<Document>)> {
    if rows.len() > 200 || rows.is_empty() {
        return Err(Error::EntityMetadataOutOfRange("批量草稿页长必须为1至200"));
    }

    let batch = &rows[0].batch_id;
    let (mut filters, mut branches, mut ids) = (Vec::new(), Vec::new(), HashSet::new());
    for row in rows {
        if row.batch_id != *batch || row.base.is_deleted() || !ids.insert(&row.base.id) {
            return Err(Error::OptimisticLockingError);
        }
        let version =
            i64::try_from(row.base.version).map_err(|_| Error::EntityMetadataOutOfRange("version"))?;
        let next = version.checked_add(1).ok_or(Error::EntityMetadataOutOfRange("version"))?;
        let mut value = serialize_to_document(row)?;
        value.insert("version", next);
        value.insert("updated_at", now);
        filters.push(doc! {"id":&row.base.id,"version":version});
        branches.push(doc! {"case":{"$eq":["$id",{"$literal":&row.base.id}]},"then":{"$literal":value}});
    }

    Ok((
        doc! {"batch_id":batch,"deleted_at":0_i64,"$or":filters},
        vec![
            doc! {"$replaceWith":{"$mergeObjects":["$$ROOT",{"$switch":{"branches":branches,"default":"$$ROOT"}}]}},
        ],
    ))
}

/// 登记请求去重及后台扫描索引。
/// # 参数
/// 当前数据库。
/// # 返回
/// 索引创建完成。
/// # 错误
/// 数据库或存量唯一性冲突。
pub async fn ensure_work_indexes(db: &Database) -> Result<()> {
    db.collection::<Document>(WORK)
        .create_index(unique_index("uk_catalog_adoption_work_request", doc! {"id":1}))
        .await?;
    db.collection::<Document>("supplier_catalog_adoption_batches")
        .create_index(named_index("idx_catalog_adoption_work", doc! {"work.status":1,"id":1}))
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "work_repository_tests.rs"]
mod tests;
