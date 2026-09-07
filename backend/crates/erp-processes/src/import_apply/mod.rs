//! Named import-apply process: batch apply, execution, confirmation and supersede.

use mongodb::Database;

mod batch;
mod complete;
mod confirmation_query;
mod create_batch;
mod create_confirmation;
pub mod dto;
mod execution;
pub mod factories;
mod supersede;

#[cfg(test)]
mod confirmation_tests;
#[cfg(test)]
mod port_failure_tests;

const IMPORT_CONFIRMATION_OBJECT_TYPE: &str = "LEGACY_IMPORT_BATCH";
const IMPORT_CONFIRMATION_HANDLER: &str = "import_business_confirmation";
const IMPORT_CONFIRMATION_WORKSPACE: &str = "W18";
const IMPORT_CONFIRMATION_ORGANIZATION: &str = "company";
const IMPORT_CONFIRMATION_AUDIT_PREFIX: &str = "import-confirmation-command-";
const IMPORT_EXECUTION_AUDIT_PREFIX: &str = "import-execution-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

/// Cross-domain import apply process service.
///
/// Holds the root transaction for apply, execution and confirmation commands.
pub struct ImportApplyService {
    db: Database,
}

impl ImportApplyService {
    /// Create an import-apply process bound to `db`.
    ///
    /// # Parameters
    /// * `db` - MongoDB database handle shared by domain repositories
    ///
    /// # Returns
    /// Process service that reuses one Executor per command.
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

/// Process module name.
pub fn process_name() -> &'static str {
    "import_apply"
}
