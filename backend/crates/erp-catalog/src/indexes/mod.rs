//! Catalog collection indexes.

mod catalog;

mod handover_receipt;

use mongodb::Database;
use persistence_core::Result;

/// Create catalog collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    catalog::ensure(db).await?;
    handover_receipt::ensure(db).await
}
