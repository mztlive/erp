//! 支撑领域集合索引。

mod bulk_job;
mod file_asset;
mod source_registry;

use mongodb::bson::Document;
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

/// 创建支撑领域各集合的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 返回
/// 三个子域索引全部创建成功时无返回值。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
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
