//! Support collection indexes.

mod bulk_job;
mod file_asset;
mod source_registry;

use mongodb::Database;
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
