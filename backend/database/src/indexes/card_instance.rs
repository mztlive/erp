//! 域 D28 `card_instance` 的索引声明：mall_consumption_cutover、mall_card_instance(+_correction)、
//! mall_balance_snapshot。
//!
//! 集合名常量取 `CardInstanceExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! §6.17 逐条对照：
//! - `mall_consumption_cutover`：「每个商城只能有一个已启用 `T`」→
//!   `uk_mall_consumption_cutovers_mall` 全局唯一（身份类字段全局唯一，与 accounts
//!   code 处理一致：软删除后仍保留身份，避免复用破坏 `T` 语义）；
//! - `mall_card_instance`：「`(mall_id, opaque_instance_ref)` 唯一」→
//!   `uk_mall_card_instances_identity`；「非空 `(mall_id, opaque_instance_ref,
//!   source_baseline_version)` 用于版本冲突校验」→ `idx_mall_card_instances_baseline_version`；
//! - `mall_card_instance_correction`：「`(mall_card_instance_id, correction_no)` 唯一」→
//!   `uk_mall_card_instance_corrections_no`；「非空 `supersedes_correction_id` 唯一」→
//!   `uk_mall_card_instance_corrections_supersedes`（稀疏唯一）；
//! - `mall_balance_snapshot`：「`(mall_card_instance_id, snapshot_at)` 始终作为业务唯一键」→
//!   `uk_mall_balance_snapshots_business`；「非空 `(mall_card_instance_id,
//!   source_snapshot_version)` 唯一并参与冲突校验」→
//!   `uk_mall_balance_snapshots_source_version`（稀疏唯一）。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::CardInstanceExt;
use crate::Result;

/// `mall_consumption_cutover` 集合名。
pub(crate) const MALL_CONSUMPTION_CUTOVERS: &str =
    <mongodb::Database as CardInstanceExt>::MALL_CONSUMPTION_CUTOVERS;
/// `mall_card_instance` 集合名。
pub(crate) const MALL_CARD_INSTANCES: &str = <mongodb::Database as CardInstanceExt>::MALL_CARD_INSTANCES;
/// `mall_card_instance_correction` 集合名。
pub(crate) const MALL_CARD_INSTANCE_CORRECTIONS: &str =
    <mongodb::Database as CardInstanceExt>::MALL_CARD_INSTANCE_CORRECTIONS;
/// `mall_balance_snapshot` 集合名。
pub(crate) const MALL_BALANCE_SNAPSHOTS: &str =
    <mongodb::Database as CardInstanceExt>::MALL_BALANCE_SNAPSHOTS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.17「必需约束与索引」；唯一约束一律用唯一索引表达。
/// 两个「非空才唯一」的引用字段（纠错链前驱、快照来源版本）用**稀疏唯一索引**
/// 表达：MongoDB 非稀疏唯一索引会把缺失字段视为 `null` 且只允许一个文档为空，
/// 而本域绝大多数记录恰好没有这两个字段，必须 `sparse` 才能表达「非空唯一」。
/// 回滚方式：删除稀疏唯一索引，改由唯一序号索引 + P3 链校验兜底。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, MALL_CONSUMPTION_CUTOVERS, cutover_indexes()).await?;
    create_indexes(db, MALL_CARD_INSTANCES, card_instance_indexes()).await?;
    create_indexes(db, MALL_CARD_INSTANCE_CORRECTIONS, correction_indexes()).await?;
    create_indexes(db, MALL_BALANCE_SNAPSHOTS, balance_snapshot_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `mall_consumption_cutover` 的身份约束索引。
fn cutover_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_mall_consumption_cutovers_mall",
        doc! { "mall_id": 1 },
    )]
}

/// 返回 `mall_card_instance` 的身份约束与基线版本查询索引。
fn card_instance_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_card_instances_identity",
            doc! { "mall_id": 1, "opaque_instance_ref": 1 },
        ),
        named_index(
            "idx_mall_card_instances_baseline_version",
            doc! { "mall_id": 1, "opaque_instance_ref": 1, "source_baseline_version": 1 },
        ),
    ]
}

/// 返回 `mall_card_instance_correction` 的纠错链约束索引。
fn correction_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_card_instance_corrections_no",
            doc! { "mall_card_instance_id": 1, "correction_no": 1 },
        ),
        sparse_unique_index(
            "uk_mall_card_instance_corrections_supersedes",
            doc! { "supersedes_correction_id": 1 },
        ),
    ]
}

/// 返回 `mall_balance_snapshot` 的业务唯一与来源版本约束索引。
fn balance_snapshot_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_mall_balance_snapshots_business",
            doc! { "mall_card_instance_id": 1, "snapshot_at": 1 },
        ),
        sparse_unique_index(
            "uk_mall_balance_snapshots_source_version",
            doc! { "mall_card_instance_id": 1, "source_snapshot_version": 1 },
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

/// 构建命名稀疏唯一索引（只对存在的字段施加唯一约束）。
fn sparse_unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .sparse(true)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{doc, Bson};

    use super::{balance_snapshot_indexes, card_instance_indexes, correction_indexes, cutover_indexes};

    #[test]
    fn cutover_mall_identity_is_globally_unique() {
        let indexes = cutover_indexes();
        let mall = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_consumption_cutovers_mall")
            })
            .unwrap();
        assert_eq!(mall.keys, doc! { "mall_id": 1 });
        assert_eq!(mall.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn card_instance_identity_is_unique_and_version_check_indexed() {
        let indexes = card_instance_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_card_instances_identity")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "mall_id": 1, "opaque_instance_ref": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "mall_id": 1, "opaque_instance_ref": 1, "source_baseline_version": 1 }
        }));
    }

    #[test]
    fn correction_indexes_cover_no_and_sparse_supersedes() {
        let indexes = correction_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_card_instance_corrections_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "mall_card_instance_id": 1, "correction_no": 1 }
        }));

        let supersedes = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_card_instance_corrections_supersedes")
            })
            .unwrap();
        assert_eq!(supersedes.keys, doc! { "supersedes_correction_id": 1 });
        assert_eq!(supersedes.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(supersedes.options.as_ref().unwrap().sparse, Some(true));
        assert!(matches!(
            supersedes.keys.get("supersedes_correction_id"),
            Some(Bson::Int32(1))
        ));
    }

    #[test]
    fn balance_snapshot_indexes_cover_business_key_and_sparse_version() {
        let indexes = balance_snapshot_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_mall_balance_snapshots_business")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "mall_card_instance_id": 1, "snapshot_at": 1 }
        }));

        let source_version = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_mall_balance_snapshots_source_version")
            })
            .unwrap();
        assert_eq!(
            source_version.keys,
            doc! { "mall_card_instance_id": 1, "source_snapshot_version": 1 }
        );
        assert_eq!(source_version.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(source_version.options.as_ref().unwrap().sparse, Some(true));
    }
}
