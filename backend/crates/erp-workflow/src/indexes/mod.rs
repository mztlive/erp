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
