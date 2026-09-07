//! Composition helper for processes that still construct RBAC.

use erp_identity::SharedRbacService;
use mongodb::Database;

use super::identity_audit::MongoIdentityAudit;

/// Compose identity RBAC with the audit adapter for processes.
///
/// # Parameters
/// * `db` - MongoDB database
///
/// # Returns
/// Shared RBAC service that persists identity audits through `erp-audit`.
pub fn shared_rbac_service(db: Database) -> SharedRbacService {
    let audit = MongoIdentityAudit::shared(db.clone());
    erp_identity::shared_rbac_service(db, audit)
}
