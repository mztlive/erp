//! 支撑领域集合索引。

mod bulk_job;
mod file_asset;
mod source_registry;

use mongodb::bson::Document;
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

/// 创建支撑领域各集合的幂等命名索引。
/// 全部支撑域索引创建成功时无返回值。
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    source_registry::ensure(db).await?;
    bulk_job::ensure(db).await?;
    scheduled_runs::ensure(db).await?;
    file_asset::ensure(db).await?;
    Ok(())
}

// 启动组合根按基线交错顺序使用单一领域索引实现。
pub use bulk_job::ensure as ensure_bulk_job;
pub use file_asset::ensure as ensure_file_asset;
pub use source_registry::ensure as ensure_source_registry;

// 为单个集合创建一组幂等命名索引。
// `collection` 为目标集合名（取各域 `*Ext` 关联常量）；`indexes` 为待创建的索引模型。
// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

mod scheduled_runs;
pub use scheduled_runs::ensure as ensure_scheduled_runs;
