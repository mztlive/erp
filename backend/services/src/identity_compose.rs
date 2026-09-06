//! Composition helper for remaining unmigrated services that still construct RBAC.

use erp_identity::SharedRbacService;
use mongodb::Database;

use crate::identity_audit::MongoIdentityAudit;

/// Compose identity RBAC with the audit adapter for remaining unmigrated services.
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
