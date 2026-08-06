//! 域 D04 `bulk_job` 的索引声明：bulk_selection_snapshot、bulk_selection_item、background_job、background_job_item。
//!
//! 集合名常量取 `BulkJobExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::BulkJobExt;
use crate::Result;

/// `bulk_selection_snapshot` 集合名。
pub(crate) const BULK_SELECTION_SNAPSHOTS: &str = <mongodb::Database as BulkJobExt>::BULK_SELECTION_SNAPSHOTS;
/// `bulk_selection_item` 集合名。
pub(crate) const BULK_SELECTION_ITEMS: &str = <mongodb::Database as BulkJobExt>::BULK_SELECTION_ITEMS;
/// `background_job` 集合名。
pub(crate) const BACKGROUND_JOBS: &str = <mongodb::Database as BulkJobExt>::BACKGROUND_JOBS;
/// `background_job_item` 集合名。
pub(crate) const BACKGROUND_JOB_ITEMS: &str = <mongodb::Database as BulkJobExt>::BACKGROUND_JOB_ITEMS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.1「必需约束与索引」：
/// - `bulk_selection_item`：`(selection_snapshot_id, object_type, object_id)` 唯一；
/// - `background_job`：`job_no`、`request_id` 分别唯一；
/// - `background_job_item`：`(background_job_id, item_no)` 唯一；
/// - 快照/任务按创建人、状态与有效期查询索引（W02、W18 工作台）。
///
/// 身份类字段全局唯一（与 accounts 处理一致）：软删除后仍保留身份，避免
/// 复用破坏恢复语义。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, BULK_SELECTION_SNAPSHOTS, bulk_selection_snapshot_indexes()).await?;
    create_indexes(db, BULK_SELECTION_ITEMS, bulk_selection_item_indexes()).await?;
    create_indexes(db, BACKGROUND_JOBS, background_job_indexes()).await?;
    create_indexes(db, BACKGROUND_JOB_ITEMS, background_job_item_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `bulk_selection_snapshot` 的身份约束与工作台查询索引。
///
/// 快照是一次性工作结构，无自然业务唯一键，用 `id` 唯一索引防止重复身份
/// 静默写入；`created_by + created_at` 覆盖「我的快照」列表，`status + expires_at`
/// 覆盖待办与过期扫描（W02）。
fn bulk_selection_snapshot_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_bulk_selection_snapshots_id", doc! { "id": 1 }),
        named_index(
            "idx_bulk_selection_snapshots_created",
            doc! { "created_by": 1, "created_at": -1 },
        ),
        named_index(
            "idx_bulk_selection_snapshots_status_expires",
            doc! { "status": 1, "expires_at": 1 },
        ),
    ]
}

/// 返回 `bulk_selection_item` 的目标唯一约束与结果筛选索引。
fn bulk_selection_item_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_bulk_selection_items_target",
            doc! {
                "selection_snapshot_id": 1,
                "object_type": 1,
                "object_id": 1,
            },
        ),
        named_index(
            "idx_bulk_selection_items_result",
            doc! { "selection_snapshot_id": 1, "result_status": 1 },
        ),
    ]
}

/// 返回 `background_job` 的身份约束与任务中心查询索引。
fn background_job_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_background_jobs_no", doc! { "job_no": 1 }),
        unique_index("uk_background_jobs_request_id", doc! { "request_id": 1 }),
        named_index(
            "idx_background_jobs_status_created",
            doc! { "status": 1, "created_at": -1 },
        ),
        named_index(
            "idx_background_jobs_domain",
            doc! { "domain_job_type": 1, "domain_job_id": 1 },
        ),
        named_index(
            "idx_background_jobs_requested_created",
            doc! { "requested_by": 1, "created_at": -1 },
        ),
    ]
}

/// 返回 `background_job_item` 的逐项唯一约束与结果筛选索引。
fn background_job_item_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_background_job_items_no",
            doc! { "background_job_id": 1, "item_no": 1 },
        ),
        named_index(
            "idx_background_job_items_status",
            doc! { "background_job_id": 1, "status": 1 },
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

    use super::{
        background_job_indexes, background_job_item_indexes, bulk_selection_item_indexes,
        bulk_selection_snapshot_indexes,
    };

    #[test]
    fn snapshot_identity_is_unique_and_workbench_indexes_exist() {
        let indexes = bulk_selection_snapshot_indexes();

        let id_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_bulk_selection_snapshots_id")
            })
            .unwrap();
        assert_eq!(id_index.options.as_ref().unwrap().unique, Some(true));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "created_by": 1, "created_at": -1 }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "status": 1, "expires_at": 1 }));
    }

    #[test]
    fn selection_item_target_is_unique_within_snapshot() {
        let indexes = bulk_selection_item_indexes();

        let target = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_bulk_selection_items_target")
            })
            .unwrap();
        assert_eq!(target.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            target.keys,
            doc! {
                "selection_snapshot_id": 1,
                "object_type": 1,
                "object_id": 1,
            }
        );
    }

    #[test]
    fn background_job_identity_indexes_are_globally_unique() {
        let indexes = background_job_indexes();

        for name in ["uk_background_jobs_no", "uk_background_jobs_request_id"] {
            let index = indexes
                .iter()
                .find(|index| {
                    index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                })
                .unwrap();
            let options = index.options.as_ref().unwrap();
            assert_eq!(options.unique, Some(true));
            assert!(options.partial_filter_expression.is_none());
        }
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "status": 1, "created_at": -1 }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "domain_job_type": 1, "domain_job_id": 1 }));
    }

    #[test]
    fn background_job_item_number_is_unique_within_job() {
        let indexes = background_job_item_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_background_job_items_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "background_job_id": 1, "status": 1 }));
    }
}
