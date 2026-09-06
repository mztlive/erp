//! Warehouse collection indexes.

mod warehouse;

use mongodb::Database;
use persistence_core::Result;

/// Create warehouse collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    warehouse::ensure(db).await
}
