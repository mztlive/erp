//! 域 D05 `file_asset` 的索引声明：file_asset、document_attachment。
//!
//! 集合名常量取 `FileAssetExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::FileAssetExt;
use crate::Result;

/// `file_asset` 集合名。
pub(crate) const FILE_ASSETS: &str = <mongodb::Database as FileAssetExt>::FILE_ASSETS;
/// `document_attachment` 集合名。
pub(crate) const DOCUMENT_ATTACHMENTS: &str = <mongodb::Database as FileAssetExt>::DOCUMENT_ATTACHMENTS;

/// 创建本域集合的幂等命名索引。
///
/// 落地数据模型 §6.1 / §4.5.7：
/// - `file_asset`：`storage_object_key` 是加密受控对象存储中的不可猜测对象键，
///   全局唯一（身份字段，软删除后仍保留，避免复用破坏销毁审计链）；
///   `security_scan_status + retention_class` 覆盖扫描队列与保留策略筛选；
///   `expires_at` 覆盖到期资产查询；
/// - `document_attachment`：追加式关联（§4.5.7 审计留痕）无自然业务唯一键，
///   用 `id` 唯一索引防止重复身份静默写入；按单据与按资产的正反查询索引。
///
/// 到期清理说明：实体 `Instant` 按 P0 固化的 Int64（秒级时间戳）持久化，而
/// MongoDB TTL 索引要求索引字段为 BSON Date，**无法直接对 `expires_at` 建 TTL
/// 索引**（§4.5.7 导出结果保留 7 天 / 失败诊断 30 天）。本域以
/// `idx_file_assets_expires_at` 提供到期查询索引，物理销毁由 P5 后台任务按
/// `expires_at` 扫描并标记 `destroyed_at`；将 `expires_at` 改为 BSON Date 需要
/// P0 地基修订（entity `Instant` 序列化形态），不在本阶段冻结范围内。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, FILE_ASSETS, file_asset_indexes()).await?;
    create_indexes(db, DOCUMENT_ATTACHMENTS, document_attachment_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `file_asset` 的身份约束、扫描队列与到期查询索引。
fn file_asset_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_file_assets_storage_key", doc! { "storage_object_key": 1 }),
        named_index(
            "idx_file_assets_scan_retention",
            doc! { "security_scan_status": 1, "retention_class": 1 },
        ),
        named_index("idx_file_assets_expires_at", doc! { "expires_at": 1 }),
    ]
}

/// 返回 `document_attachment` 的身份约束与单据/资产查询索引。
fn document_attachment_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_document_attachments_id", doc! { "id": 1 }),
        named_index(
            "idx_document_attachments_document",
            doc! { "document_id": 1, "created_at": 1 },
        ),
        named_index("idx_document_attachments_asset", doc! { "file_asset_id": 1 }),
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

    use super::{document_attachment_indexes, file_asset_indexes};

    #[test]
    fn storage_key_identity_is_globally_unique() {
        let indexes = file_asset_indexes();

        let storage_key = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_file_assets_storage_key")
            })
            .unwrap();
        assert_eq!(storage_key.keys, doc! { "storage_object_key": 1 });
        assert_eq!(storage_key.options.as_ref().unwrap().unique, Some(true));
        assert!(storage_key
            .options
            .as_ref()
            .unwrap()
            .partial_filter_expression
            .is_none());

        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "security_scan_status": 1, "retention_class": 1 } }));
        assert!(indexes.iter().any(|index| index.keys == doc! { "expires_at": 1 }));
    }

    #[test]
    fn attachment_indexes_cover_id_and_both_lookup_directions() {
        let indexes = document_attachment_indexes();

        let id_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_document_attachments_id")
            })
            .unwrap();
        assert_eq!(id_index.options.as_ref().unwrap().unique, Some(true));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "document_id": 1, "created_at": 1 } }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "file_asset_id": 1 }));
    }
}
