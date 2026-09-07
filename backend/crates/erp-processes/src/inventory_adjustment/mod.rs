//! Named inventory-adjustment process: create, submit, cancel and post.

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_workflow::ApprovalObjectReadPort;
use mongodb::Database;

mod adapter;
mod approval_prepare;
mod approval_query;
pub mod cancel_approval;
mod create;
mod mapping;
mod persist;
mod post;
mod query;
mod submit;

#[cfg(test)]
mod start_tests;

/// Cross-domain inventory adjustment process service.
///
/// Holds the root transaction for submit/cancel/create and reuses the caller
/// Executor for approval-runtime post and cancel actions.
pub struct InventoryAdjustmentService {
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
}

impl InventoryAdjustmentService {
    /// Create an inventory-adjustment process bound to `db`.
    ///
    /// # Parameters
    /// * `db` - MongoDB database handle shared by domain repositories
    /// * `rbac` - shared RBAC used by approval binding and authorization
    ///
    /// # Returns
    /// Process service that reuses one Executor per command.
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self {
            db,
            rbac,
            object_read: Arc::new(erp_workflow::FailClosedObjectReadPort),
        }
    }

    /// Inject composition-root object-read for approval binding.
    pub fn with_object_read(mut self, object_read: Arc<dyn ApprovalObjectReadPort>) -> Self {
        self.object_read = object_read;
        self
    }
}

/// Process module name.
pub fn process_name() -> &'static str {
    "inventory_adjustment"
}
