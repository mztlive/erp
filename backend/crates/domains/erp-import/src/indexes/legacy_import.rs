//! 域 D22 `legacy_import` 的索引声明：legacy_import_batch、legacy_import_row、legacy_import_confirmation。
//!
//! 集合名常量取 `LegacyImportExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::extensions::LegacyImportExt;

/// 失败诊断保留天数（数据模型 §4.5.7/§6.12：失败合规包与行列诊断明细 30 天清理）。
const DIAGNOSTIC_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

/// `legacy_import_batch` 集合名。
pub(crate) const LEGACY_IMPORT_BATCHES: &str = <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_BATCHES;
/// `legacy_import_row` 集合名。
pub(crate) const LEGACY_IMPORT_ROWS: &str = <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_ROWS;
/// `legacy_import_confirmation` 集合名。
pub(crate) const LEGACY_IMPORT_CONFIRMATIONS: &str =
    <mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_CONFIRMATIONS;

// 幂等注册批次、行、确认三类集合索引。身份采用全局唯一约束，软删除仍占用原身份以维持来源追溯与恢复；失败诊断及独立诊断资产遵循 30 天保留约定，批次元数据、计数、成功结果和映射审计长期保留。数据冲突或 MongoDB 创建失败时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, LEGACY_IMPORT_BATCHES, legacy_import_batch_indexes()).await?;
    create_indexes(db, LEGACY_IMPORT_ROWS, legacy_import_row_indexes()).await?;
    create_indexes(db, LEGACY_IMPORT_CONFIRMATIONS, legacy_import_confirmation_indexes()).await?;
    Ok(())
}

// 对单一导入集合注册一组幂等命名索引，供批次、行和确认复用相同创建逻辑。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

// 声明批次 ID、批次号唯一性和列表查询索引，支持稳定定位和筛选批次。
fn legacy_import_batch_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_legacy_import_batches_id", doc! { "id": 1 }),
        unique_index("uk_legacy_import_batches_batch_no", doc! { "batch_no": 1 }),
        named_index(
            "idx_legacy_import_batches_reimport_warning",
            doc! { "source_file_hmac": 1, "source_object_set": 1, "baseline_date": 1 },
        ),
    ]
}

// 声明行身份、处理队列与 30 天保留期的索引。`ttl_legacy_import_rows_diagnostics_30d` 建在公共 `created_at` 上，但 `BaseModel.created_at`（`u64`）和 `Instant`（`i64`）均以秒级整数保存，MongoDB 不会按 BSON Date TTL 清理这些记录。
// 当前公共 `created_at` TTL 不做状态筛选，只声明保留期；成功行须长期保留，失败诊断由归档任务清理。若要按失败行精确过期，须先引入 BSON Date 失败时间字段。
fn legacy_import_row_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_legacy_import_rows_id", doc! { "id": 1 }),
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
        named_index("idx_legacy_import_rows_batch_id_created", doc! { "batch_id": 1, "created_at": -1 }),
        ttl_index(
            "ttl_legacy_import_rows_diagnostics_30d",
            doc! { "created_at": 1 },
            DIAGNOSTIC_RETENTION_SECONDS,
        ),
    ]
}

// 声明确认记录、任务与范围试算的身份约束及批次矩阵查询索引。
fn legacy_import_confirmation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_legacy_import_confirmations_id", doc! { "id": 1 }),
        unique_index(
            "uk_legacy_import_confirmations_scope_trial",
            doc! {
                "batch_id": 1,
                "confirmation_scope": 1,
                "trial_version": 1,
            },
        ),
        unique_index("uk_legacy_import_confirmations_work_item", doc! { "work_item_id": 1 }),
        named_index("idx_legacy_import_confirmations_batch_status", doc! { "batch_id": 1, "status": 1 }),
    ]
}

// 构造指定过期秒数的命名 TTL 索引，统一声明诊断数据保留期。
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
    // 验证各集合都有 `id` 唯一索引，保证按稳定主键读取与乐观锁更新不用扫描集合。
    #[test]
    fn collections_look_up_by_unique_stable_id() {
        for indexes in [
            super::legacy_import_batch_indexes(),
            super::legacy_import_confirmation_indexes(),
            super::legacy_import_row_indexes(),
        ] {
            assert!(indexes.iter().any(|index| {
                index.keys == mongodb::bson::doc! { "id": 1 }
                    && index.options.as_ref().and_then(|options| options.unique) == Some(true)
            }));
        }
    }

    use mongodb::bson::doc;

    use super::{
        DIAGNOSTIC_RETENTION_SECONDS, legacy_import_batch_indexes, legacy_import_confirmation_indexes,
        legacy_import_row_indexes,
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
        assert_eq!(identity.keys, doc! { "batch_id": 1, "source_object_type": 1, "source_row_key": 1 });
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
        assert!(indexes.iter().any(|index| index.keys == doc! { "batch_id": 1, "status": 1 }));
    }
}
