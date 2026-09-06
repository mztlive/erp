//! Contract collection indexes.

mod contract;

use mongodb::Database;
use persistence_core::Result;

/// Create contract collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    contract::ensure(db).await
}
