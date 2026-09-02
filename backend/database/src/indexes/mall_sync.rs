//! 域 D23 `mall_sync` 的索引声明：mall_sales_sync_job、mall_sales_sync_cursor、
//! mall_sales_order_snapshot、mall_sales_reconciliation_job(+_item)、master_mapping_task。
//!
//! 集合名常量取 `MallSyncExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::MallSyncExt;
use crate::Result;

/// `mall_sales_sync_job` 集合名。
pub(crate) const MALL_SALES_SYNC_JOBS: &str = <mongodb::Database as MallSyncExt>::MALL_SALES_SYNC_JOBS;
/// `mall_sales_sync_cursor` 集合名。
pub(crate) const MALL_SALES_SYNC_CURSORS: &str = <mongodb::Database as MallSyncExt>::MALL_SALES_SYNC_CURSORS;
/// `mall_sales_order_snapshot` 集合名。
pub(crate) const MALL_SALES_ORDER_SNAPSHOTS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOTS;
/// `mall_sales_reconciliation_job` 集合名。
pub(crate) const MALL_SALES_RECONCILIATION_JOBS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SALES_RECONCILIATION_JOBS;
/// `mall_sales_reconciliation_item` 集合名。
pub(crate) const MALL_SALES_RECONCILIATION_ITEMS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SALES_RECONCILIATION_ITEMS;
/// `master_mapping_task` 集合名。
pub(crate) const MASTER_MAPPING_TASKS: &str = <mongodb::Database as MallSyncExt>::MASTER_MAPPING_TASKS;
/// `mall_snapshot_reapply_operation` 集合名。
pub(crate) const MALL_SNAPSHOT_REAPPLY_OPERATIONS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SNAPSHOT_REAPPLY_OPERATIONS;
/// `mall_sales_order_snapshot_watermarks` 集合名。
pub(crate) const MALL_SALES_ORDER_SNAPSHOT_WATERMARKS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SALES_ORDER_SNAPSHOT_WATERMARKS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.13「必需约束与索引」。身份类字段使用**全局唯一索引**
/// （与 accounts 的 code 处理一致）：软删除后仍保留身份，避免复用破坏
/// 来源追溯与恢复语义。
///
/// `mall_sales_order_snapshot` 的 `business_fact_key` 去重由
/// `uk_mall_sales_order_snapshots_fact_key` 唯一索引保证（P2 计划 §5：
/// 事实类去重必须靠唯一索引，不靠应用层查重）。
///
/// `uk_master_mapping_tasks_snapshot_type_pending` 是**部分唯一索引**
/// （`status = "pending"` 时才唯一）：§6.13 要求「同一快照、映射类型只允许
/// 一个进行中任务」，而已解决/关闭的任务须长期保留供历史追溯，同一
/// (快照, 类型) 在任务终结后可再次创建新任务。回滚方式：删除该索引并
/// 回退为应用层先查后插（存在重复创建窗口，仅作应急）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, MALL_SALES_SYNC_JOBS, mall_sales_sync_job_indexes()).await?;
    create_indexes(db, MALL_SALES_SYNC_CURSORS, mall_sales_sync_cursor_indexes()).await?;
    create_indexes(
        db,
        MALL_SALES_ORDER_SNAPSHOTS,
        mall_sales_order_snapshot_indexes(),
    )
    .await?;
    create_indexes(
        db,
        MALL_SALES_RECONCILIATION_JOBS,
        mall_sales_reconciliation_job_indexes(),
    )
    .await?;
    create_indexes(
        db,
        MALL_SALES_RECONCILIATION_ITEMS,
        mall_sales_reconciliation_item_indexes(),
    )
    .await?;
    create_indexes(db, MASTER_MAPPING_TASKS, master_mapping_task_indexes()).await?;
    create_indexes(
        db,
        MALL_SNAPSHOT_REAPPLY_OPERATIONS,
        mall_snapshot_reapply_operation_indexes(),
    )
    .await?;
    create_indexes(
        db,
        MALL_SALES_ORDER_SNAPSHOT_WATERMARKS,
        mall_sales_order_snapshot_watermark_indexes(),
    )
    .await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `mall_sales_sync_job` 的任务查询索引（§6.13：`source_system_id + started_at`）。
fn mall_sales_sync_job_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_mall_sales_sync_jobs_source_started",
        doc! { "source_system_id": 1, "started_at": -1 },
    )]
}

/// 返回 `mall_sales_sync_cursor` 的单行约束索引。
///
/// 每个来源商城一个当前水位（§6.13）：`source_system_id` 全局唯一。
fn mall_sales_sync_cursor_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_mall_sales_sync_cursors_source",
        doc! { "source_system_id": 1 },
    )]
}

/// 返回 `mall_sales_order_snapshot` 的事实键与处理索引。
///
/// `uk_mall_sales_order_snapshots_fact_key`：`business_fact_key`（§6.13 即
/// `(source_system_id, external_order_key, source_updated_at)`）唯一，重复
/// 推送时保留最新快照；`external_order_key` 以 BSON Binary 持久化，唯一索引
/// 直接建在该字段上。
fn mall_sales_order_snapshot_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_sales_order_snapshots_fact_key",
            doc! {
                "source_system_id": 1,
                "external_order_key": 1,
                "source_updated_at": 1,
            },
        ),
        named_index(
            "idx_mall_sales_order_snapshots_incremental",
            doc! { "source_system_id": 1, "source_updated_at": 1, "external_order_key": 1 },
        ),
        named_index(
            "idx_mall_sales_order_snapshots_difference",
            doc! { "mapping_status": 1, "observed_at": 1 },
        ),
    ]
}

/// 返回 `mall_sales_reconciliation_job` 的身份约束与查询索引。
fn mall_sales_reconciliation_job_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_mall_sales_reconciliation_jobs_job_no", doc! { "job_no": 1 }),
        named_index(
            "idx_mall_sales_reconciliation_jobs_source_asof",
            doc! { "source_system_id": 1, "source_list_as_of": -1 },
        ),
    ]
}

/// 返回 `mall_sales_reconciliation_item` 的身份约束与查询索引。
fn mall_sales_reconciliation_item_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_sales_reconciliation_items_job_key",
            doc! {
                "reconciliation_job_id": 1,
                "external_order_key": 1,
            },
        ),
        named_index(
            "idx_mall_sales_reconciliation_items_job_status",
            doc! { "reconciliation_job_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `master_mapping_task` 的进行中唯一约束与待办索引。
fn master_mapping_task_indexes() -> Vec<IndexModel> {
    vec![
        partial_unique_index(
            "uk_master_mapping_tasks_snapshot_type_pending",
            doc! { "source_snapshot_id": 1, "mapping_type": 1 },
            doc! { "status": "pending" },
        ),
        named_index(
            "idx_master_mapping_tasks_todo",
            doc! { "owner_role": 1, "status": 1, "created_at": 1 },
        ),
        named_index(
            "idx_master_mapping_tasks_snapshot",
            doc! { "source_snapshot_id": 1, "created_at": -1 },
        ),
    ]
}

/// 返回来源单快照单调水位索引。
///
/// `uk_mall_sales_order_snapshot_watermarks_order`：每个
/// `(source_system_id, external_order_key)` 一行，配合 `$lt` CAS 阻止并发
/// 旧版本落盘。exact 唯一索引只防同时键，不能替代本单调约束。
/// 前向：`ensure_indexes` 幂等创建；回滚：删除该索引及集合，查询退化为无水位
/// （并发旧版本可能再次落盘，仅作应急）。
fn mall_sales_order_snapshot_watermark_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_mall_sales_order_snapshot_watermarks_order",
        doc! { "source_system_id": 1, "external_order_key": 1 },
    )]
}

/// 返回重新归集操作的幂等与任务时间线索引。
fn mall_snapshot_reapply_operation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_snapshot_reapply_operations_task_idempotency",
            doc! { "mapping_task_id": 1, "idempotency_key_hash": 1 },
        ),
        named_index(
            "idx_mall_snapshot_reapply_operations_task_updated",
            doc! { "mapping_task_id": 1, "last_updated_at": -1 },
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

/// 构建命名部分唯一索引（`partial_filter_expression` 命中时才唯一）。
fn partial_unique_index(
    name: impl Into<String>,
    keys: Document,
    partial_filter_expression: Document,
) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .partial_filter_expression(partial_filter_expression)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        mall_sales_order_snapshot_indexes, mall_sales_order_snapshot_watermark_indexes,
        mall_sales_reconciliation_item_indexes, mall_sales_sync_cursor_indexes,
        mall_snapshot_reapply_operation_indexes, master_mapping_task_indexes,
    };

    #[test]
    fn reapply_operation_idempotency_is_unique_per_mapping_task() {
        let indexes = mall_snapshot_reapply_operation_indexes();
        let index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_snapshot_reapply_operations_task_idempotency")
            })
            .unwrap();
        assert_eq!(
            index.keys,
            doc! { "mapping_task_id": 1, "idempotency_key_hash": 1 }
        );
        assert_eq!(
            index.options.as_ref().and_then(|options| options.unique),
            Some(true)
        );
    }

    #[test]
    fn cursor_source_is_globally_unique() {
        let indexes = mall_sales_sync_cursor_indexes();

        let cursor = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_sales_sync_cursors_source")
            })
            .unwrap();
        assert_eq!(cursor.keys, doc! { "source_system_id": 1 });
        assert_eq!(cursor.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn snapshot_fact_key_is_unique_and_incremental_difference_indexes_exist() {
        let indexes = mall_sales_order_snapshot_indexes();

        let fact_key = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_sales_order_snapshots_fact_key")
            })
            .unwrap();
        assert_eq!(
            fact_key.keys,
            doc! {
                "source_system_id": 1,
                "external_order_key": 1,
                "source_updated_at": 1,
            }
        );
        assert_eq!(fact_key.options.as_ref().unwrap().unique, Some(true));
        // 唯一索引直接建在 external_order_key 字段（Bson 字段路径，不包表达式）。
        assert!(fact_key.keys.contains_key("external_order_key"));

        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "source_system_id": 1, "source_updated_at": 1, "external_order_key": 1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "mapping_status": 1, "observed_at": 1 } }));
    }

    #[test]
    fn snapshot_watermark_order_is_globally_unique() {
        let indexes = mall_sales_order_snapshot_watermark_indexes();
        let watermark = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_sales_order_snapshot_watermarks_order")
            })
            .unwrap();
        assert_eq!(
            watermark.keys,
            doc! { "source_system_id": 1, "external_order_key": 1 }
        );
        assert_eq!(watermark.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn reconciliation_item_job_key_is_unique_and_status_index_exists() {
        let indexes = mall_sales_reconciliation_item_indexes();

        let job_key = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_sales_reconciliation_items_job_key")
            })
            .unwrap();
        assert_eq!(
            job_key.keys,
            doc! { "reconciliation_job_id": 1, "external_order_key": 1 }
        );
        assert_eq!(job_key.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "reconciliation_job_id": 1, "status": 1 }));
    }

    #[test]
    fn mapping_task_pending_partial_unique_and_todo_index() {
        let indexes = master_mapping_task_indexes();

        let pending = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_master_mapping_tasks_snapshot_type_pending")
            })
            .unwrap();
        assert_eq!(pending.keys, doc! { "source_snapshot_id": 1, "mapping_type": 1 });
        let options = pending.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        assert_eq!(
            options.partial_filter_expression,
            Some(doc! { "status": "pending" }),
            "仅进行中任务唯一，终结任务保留后允许新任务"
        );

        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "owner_role": 1, "status": 1, "created_at": 1 } }));
    }
}
