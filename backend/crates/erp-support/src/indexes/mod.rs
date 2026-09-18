//! Support collection indexes.

mod bulk_job;
mod file_asset;
mod source_registry;

use mongodb::bson::Document;
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

/// Create support collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    source_registry::ensure(db).await?;
    bulk_job::ensure(db).await?;
    file_asset::ensure(db).await?;
    Ok(())
}

// 启动组合根按基线交错顺序使用单一领域索引实现。
pub use bulk_job::ensure as ensure_bulk_job;
pub use file_asset::ensure as ensure_file_asset;
pub use source_registry::ensure as ensure_source_registry;

/// 为单个集合创建一组幂等命名索引（三集合共用）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `collection` - 目标集合名（取各域 `*Ext` 关联常量）
/// * `indexes` - 待创建的索引模型
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

/// 构建命名普通索引（三集合共用）。
///
/// # 参数
/// * `name` - 索引名
/// * `keys` - 索引键文档
///
/// # 返回
/// 返回索引模型。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder().keys(keys).options(IndexOptions::builder().name(name.into()).build()).build()
}

/// 构建命名唯一索引（三集合共用）。
///
/// # 参数
/// * `name` - 索引名
/// * `keys` - 索引键文档
///
/// # 返回
/// 返回唯一索引模型。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}
