//! Customer collection indexes.

mod customer;

use mongodb::Database;
use persistence_core::Result;

/// Create customer collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    customer::ensure(db).await
}
