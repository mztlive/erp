//! Inventory collection indexes.

mod inventory;

use mongodb::Database;
use persistence_core::Result;

/// Create inventory collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    inventory::ensure(db).await
}
