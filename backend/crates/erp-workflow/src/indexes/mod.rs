//! Workflow collection indexes.

mod approval_integration;
mod bpm;
mod document_registry;
mod work_item;

use mongodb::Database;
use persistence_core::Result;

/// Create workflow collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
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
