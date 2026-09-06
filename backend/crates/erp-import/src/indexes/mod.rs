//! Import collection indexes.

mod legacy_import;

use mongodb::Database;
use persistence_core::Result;

/// Create import collection indexes.
///
/// # Parameters
/// * `db` - target MongoDB database
///
/// # Errors
/// Unique-constraint violations or MongoDB index creation failures.
pub async fn ensure(db: &Database) -> Result<()> {
    legacy_import::ensure(db).await
}
