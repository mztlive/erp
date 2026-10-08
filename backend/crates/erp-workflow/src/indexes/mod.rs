//! 工作流集合索引。

mod approval_integration;
mod bpm;
mod document_registry;
mod work_item;

use mongodb::Database;
use persistence_core::Result;

/// 创建工作流各集合索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 无返回值。
///
/// # 错误
/// 任一子模块因唯一约束冲突或 MongoDB 无法创建索引而失败时返回对应错误。
pub async fn ensure(db: &Database) -> Result<()> {
    approval_integration::ensure(db).await?;
    bpm::ensure(db).await?;
    document_registry::ensure(db).await?;
    work_item::ensure(db).await?;
    Ok(())
}

// 启动组合根按基线交错顺序使用单一领域索引实现。
pub use approval_integration::ensure as ensure_approval_integration;
pub use bpm::ensure as ensure_bpm;
pub use document_registry::ensure as ensure_document_registry;
pub use work_item::ensure as ensure_work_item;
