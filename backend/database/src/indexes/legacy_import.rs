//! 域 D22 `legacy_import` 的索引声明：legacy_import_batch、legacy_import_row、legacy_import_confirmation。
//!
//! 集合名常量取 `LegacyImportExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::LegacyImportExt;
use crate::Result;

/// 失败诊断保留天数（数据模型 §4.5.7/§6.12：失败合规包与行列诊断明细 30 天清理）。
const DIAGNOSTIC_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

/// `legacy_import_batch` 集合名。
pub(crate) const LEGACY_IMPORT_BATCHES: &str = <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_BATCHES;
/// `legacy_import_row` 集合名。
pub(crate) const LEGACY_IMPORT_ROWS: &str = <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_ROWS;
/// `legacy_import_confirmation` 集合名。
pub(crate) const LEGACY_IMPORT_CONFIRMATIONS: &str =
    <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_CONFIRMATIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.12「必需约束与索引」；身份类字段使用**全局唯一索引**
/// （与 accounts 的 code 处理一致）：软删除后仍保留身份，避免复用破坏
/// 来源追溯与恢复语义。
///
/// TTL（`ttl_legacy_import_rows_diagnostics_30d`）：失败行诊断（规范化载荷、
/// 错误明细）与 `failure_diagnostic_file_asset_id` 按 30 天清理（§4.5.7/§6.12）。
/// 批次元数据、汇总计数、成功结果行与映射审计长期保留，**不建 TTL**。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, LEGACY_IMPORT_BATCHES, legacy_import_batch_indexes()).await?;
    create_indexes(db, LEGACY_IMPORT_ROWS, legacy_import_row_indexes()).await?;
    create_indexes(
        db,
        LEGACY_IMPORT_CONFIRMATIONS,
        legacy_import_confirmation_indexes(),
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

/// 返回 `legacy_import_batch` 的身份约束和查询索引。
fn legacy_import_batch_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_legacy_import_batches_batch_no", doc! { "batch_no": 1 }),
        named_index(
            "idx_legacy_import_batches_reimport_warning",
            doc! { "source_file_hmac": 1, "source_object_set": 1, "baseline_date": 1 },
        ),
    ]
}

/// 返回 `legacy_import_row` 的身份约束、处理队列与 TTL 索引。
///
/// `ttl_legacy_import_rows_diagnostics_30d`：按数据模型 §4.5.7/§6.12 落地
/// 失败诊断 30 天清理契约。注意两个平台约束（P0 冻结）：
/// 1. MongoDB TTL 索引要求字段为 BSON Date，而 P0 固定的公共时间基元
///    （`BaseModel.created_at` u64、`Instant` i64）以秒级 int64 持久化，
///    驱动不会据此清理文档，真正清理需归档任务兜底；
/// 2. TTL 索引不支持 `partialFilterExpression`，无法只对失败行生效，而
///    成功结果行必须长期保留（§6.12），因此本索引建在公共 `created_at` 上，
///    仅声明保留期契约；按行精确清理需要地基修订引入 BSON Date 的失败
///    时间字段后调整（见 P2 报告偏差）。
fn legacy_import_row_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_legacy_import_rows_batch_identity",
            doc! {
                "batch_id": 1,
                "source_object_type": 1,
                "source_row_key": 1,
            },
        ),
        named_index(
            "idx_legacy_import_rows_process_queue",
            doc! { "parse_status": 1, "mapping_status": 1, "import_status": 1 },
        ),
        named_index(
            "idx_legacy_import_rows_batch_id_created",
            doc! { "batch_id": 1, "created_at": -1 },
        ),
        ttl_index(
            "ttl_legacy_import_rows_diagnostics_30d",
            doc! { "created_at": 1 },
            DIAGNOSTIC_RETENTION_SECONDS,
        ),
    ]
}

/// 返回 `legacy_import_confirmation` 的身份约束与矩阵查询索引。
fn legacy_import_confirmation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_legacy_import_confirmations_scope_trial",
            doc! {
                "batch_id": 1,
                "confirmation_scope": 1,
                "trial_version": 1,
            },
        ),
        unique_index(
            "uk_legacy_import_confirmations_work_item",
            doc! { "work_item_id": 1 },
        ),
        named_index(
            "idx_legacy_import_confirmations_batch_status",
            doc! { "batch_id": 1, "status": 1 },
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

/// 构建命名 TTL 索引（`expire_after_seconds` 秒后过期）。
fn ttl_index(name: impl Into<String>, keys: Document, expire_after_seconds: i64) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .expire_after(Some(std::time::Duration::from_secs(expire_after_seconds as u64)))
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        legacy_import_batch_indexes, legacy_import_confirmation_indexes, legacy_import_row_indexes,
        DIAGNOSTIC_RETENTION_SECONDS,
    };

    #[test]
    fn batch_identity_and_reimport_warning_indexes() {
        let indexes = legacy_import_batch_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_legacy_import_batches_batch_no")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "batch_no": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "source_file_hmac": 1, "source_object_set": 1, "baseline_date": 1 }
        }));
        assert!(
            indexes.iter().all(|index| index
                .options
                .as_ref()
                .and_then(|options| options.expire_after)
                .is_none()),
            "批次元数据长期保留，不得带 TTL"
        );
    }

    #[test]
    fn row_identity_queue_and_ttl_diagnostic_indexes() {
        let indexes = legacy_import_row_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_legacy_import_rows_batch_identity")
            })
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! { "batch_id": 1, "source_object_type": 1, "source_row_key": 1 }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "parse_status": 1, "mapping_status": 1, "import_status": 1 }
        }));

        let ttl = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("ttl_legacy_import_rows_diagnostics_30d")
            })
            .expect("失败诊断 TTL 索引必须存在");
        assert_eq!(ttl.keys, doc! { "created_at": 1 });
        assert_eq!(
            ttl.options.as_ref().unwrap().expire_after.unwrap().as_secs(),
            DIAGNOSTIC_RETENTION_SECONDS as u64,
            "失败诊断保留 30 天"
        );
    }

    #[test]
    fn confirmation_scope_trial_and_work_item_are_unique() {
        let indexes = legacy_import_confirmation_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_legacy_import_confirmations_scope_trial")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_legacy_import_confirmations_work_item")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "batch_id": 1, "status": 1 }));
    }
}
