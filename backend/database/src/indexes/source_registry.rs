//! 域 D01 `source_registry` 的索引声明：source_system、external_identity_map、external_identity_target。
//!
//! 集合名常量取 `SourceRegistryExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SourceRegistryExt;
use crate::Result;

/// `source_system` 集合名。
pub(crate) const SOURCE_SYSTEMS: &str = <mongodb::Database as SourceRegistryExt>::SOURCE_SYSTEMS;
/// `external_identity_map` 集合名。
pub(crate) const EXTERNAL_IDENTITY_MAPS: &str =
    <mongodb::Database as SourceRegistryExt>::EXTERNAL_IDENTITY_MAPS;
/// `external_identity_target` 集合名。
pub(crate) const EXTERNAL_IDENTITY_TARGETS: &str =
    <mongodb::Database as SourceRegistryExt>::EXTERNAL_IDENTITY_TARGETS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.1「必需约束与索引」；身份类字段使用**全局唯一索引**
/// （与 accounts 的 code 处理一致）：软删除后仍保留身份，避免复用破坏
/// 来源追溯与恢复语义。`external_id_key` 以 BSON 二进制持久化，
/// 唯一索引直接建在该字段上。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SOURCE_SYSTEMS, source_system_indexes()).await?;
    create_indexes(db, EXTERNAL_IDENTITY_MAPS, external_identity_map_indexes()).await?;
    create_indexes(db, EXTERNAL_IDENTITY_TARGETS, external_identity_target_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `source_system` 的身份约束和列表查询索引。
fn source_system_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_source_systems_code", doc! { "code": 1 }),
        named_index(
            "idx_source_systems_type_status",
            doc! { "system_type": 1, "status": 1 },
        ),
    ]
}

/// 返回 `external_identity_map` 的身份约束和状态查询索引。
fn external_identity_map_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_external_identity_maps_identity",
            doc! {
                "source_system_id": 1,
                "object_type": 1,
                "external_id_key": 1,
            },
        ),
        named_index("idx_external_identity_maps_status", doc! { "mapping_status": 1 }),
    ]
}

/// 返回 `external_identity_target` 的谱系唯一约束与查询索引。
fn external_identity_target_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_external_identity_targets_link",
            doc! {
                "external_identity_map_id": 1,
                "internal_object_type": 1,
                "internal_object_id": 1,
                "relation_role": 1,
                "valid_from": 1,
            },
        ),
        named_index(
            "idx_external_identity_targets_lineage",
            doc! { "internal_object_type": 1, "internal_object_id": 1, "status": 1 },
        ),
        named_index(
            "idx_external_identity_targets_pending_conflict",
            doc! { "status": 1 },
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
    use mongodb::bson::{doc, Bson};

    use super::{external_identity_map_indexes, external_identity_target_indexes, source_system_indexes};

    #[test]
    fn source_system_identity_index_is_globally_unique() {
        let indexes = source_system_indexes();

        let code_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_source_systems_code")
            })
            .unwrap();
        assert_eq!(code_index.keys, doc! { "code": 1 });
        assert_eq!(code_index.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "system_type": 1, "status": 1 }));
    }

    #[test]
    fn external_identity_map_identity_index_covers_the_binary_key() {
        let indexes = external_identity_map_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_external_identity_maps_identity")
            })
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! { "source_system_id": 1, "object_type": 1, "external_id_key": 1 }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        // 唯一索引直接建在 external_id_key 字段（Bson 字段路径，不包表达式）。
        assert!(identity.keys.contains_key("external_id_key"));
        assert!(matches!(
            identity.keys.get("external_id_key"),
            Some(Bson::Int32(1))
        ));
    }

    #[test]
    fn external_identity_target_indexes_cover_link_lineage_and_status() {
        let indexes = external_identity_target_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_external_identity_targets_link")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "internal_object_type": 1,
                    "internal_object_id": 1,
                    "status": 1,
                }
        }));
        assert!(indexes.iter().any(|index| index.keys == doc! { "status": 1 }));
    }
}
