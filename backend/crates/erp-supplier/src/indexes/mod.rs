//! Supplier collection indexes.

mod supplier;

mod handover_receipt;

use mongodb::Database;
use persistence_core::Result;

/// Create supplier collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    supplier::ensure(db).await?;
    handover_receipt::ensure(db).await
}
