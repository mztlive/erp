//! Audit repository accessor implemented for MongoDB `Database`.

use mongodb::Database;

use crate::repository::owned::AuditLogRepository;

/// Audit-log collection accessor.
pub trait AuditExt {
    /// `audit_logs` collection name.
    const AUDIT_LOGS: &'static str = "audit_logs";

    /// Returns the audit log repository.
    fn audit_logs(&self) -> AuditLogRepository<'_>;
}

impl AuditExt for Database {
    /// Returns the audit log repository bound to `audit_logs`.
    fn audit_logs(&self) -> AuditLogRepository<'_> {
        AuditLogRepository::new(self, Self::AUDIT_LOGS)
    }
}
