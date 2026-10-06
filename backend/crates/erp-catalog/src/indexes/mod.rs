//! Catalog collection indexes.

mod barcode_claim;
mod catalog;

mod handover_receipt;
mod portal;

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
    barcode_claim::ensure(db).await?;
    handover_receipt::ensure(db).await?;
    portal::ensure(db).await
}
