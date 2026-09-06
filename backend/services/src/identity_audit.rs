//! Composition adapter: remaining services and entrypoints persist identity audits via erp-audit.

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog, AuditLogData};
use erp_identity::{IdentityAuditPort, PreparedResourceAudit};
use mongodb::Database;
use persistence_core::Executor;

/// MongoDB adapter that converts identity audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoIdentityAudit {
    db: Database,
}

impl MongoIdentityAudit {
    /// Bind the adapter to `db`.
    ///
    /// # Parameters
    /// * `db` - MongoDB database used to persist audit logs
    ///
    /// # Returns
    /// Adapter implementing [`IdentityAuditPort`].
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn IdentityAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl IdentityAuditPort for MongoIdentityAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_identity::Result<PreparedResourceAudit> {
        let log = actor
            .resource_log(action, resource_type, resource_id)
            .map_err(map_audit_error)?;
        Ok(prepared_from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedResourceAudit,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<()> {
        let log = audit_log_from_prepared(audit).map_err(map_audit_error)?;
        self.db
            .audit_logs()
            .create(&log, executor)
            .await
            .map_err(erp_identity::Error::from)?;
        Ok(())
    }
}

/// Copy constructed audit fields so persist does not regenerate timestamps.
fn prepared_from_log(log: &AuditLog) -> PreparedResourceAudit {
    PreparedResourceAudit::from_validated(
        &log.base,
        log.actor_id.clone(),
        log.actor_account.clone(),
        log.actor_type,
        log.action.clone(),
        log.resource_type.clone(),
        log.resource_id.clone(),
        log.success,
        log.message.clone(),
    )
}

/// Rebuild the audit entity with the original BaseModel snapshot.
fn audit_log_from_prepared(audit: &PreparedResourceAudit) -> erp_audit::Result<AuditLog> {
    let mut log = AuditLog::new(
        audit.id.clone(),
        AuditLogData {
            actor_id: audit.actor_id.clone(),
            actor_account: audit.actor_account.clone(),
            actor_type: audit.actor_type,
            action: audit.action.clone(),
            resource_type: audit.resource_type.clone(),
            resource_id: audit.resource_id.clone(),
            success: audit.success,
            message: audit.message.clone(),
        },
    )?;
    log.base = BaseModel {
        id: audit.id.clone(),
        version: audit.version,
        created_at: audit.created_at,
        updated_at: audit.updated_at,
        deleted_at: audit.deleted_at,
    };
    Ok(log)
}

fn map_audit_error(error: erp_audit::Error) -> erp_identity::Error {
    match error {
        erp_audit::Error::Internal(message) => erp_identity::Error::Internal(message),
        erp_audit::Error::NotFound(message) => erp_identity::Error::NotFound(message),
        erp_audit::Error::ValidationError(message) => erp_identity::Error::ValidationError(message),
        erp_audit::Error::BusinessLogicError(message) => erp_identity::Error::BusinessLogicError(message),
        erp_audit::Error::ConflictError(message) => erp_identity::Error::ConflictError(message),
        erp_audit::Error::ReceiptDuplicate(error) => erp_identity::Error::ReceiptDuplicate(error),
        erp_audit::Error::TransientTransaction(error) => erp_identity::Error::TransientTransaction(error),
        erp_audit::Error::Forbidden(message) => erp_identity::Error::Forbidden(message),
        erp_audit::Error::Unauthenticated(message) => erp_identity::Error::Unauthenticated(message),
        erp_audit::Error::Logic(error) => erp_identity::Error::Logic(error),
        erp_audit::Error::OutcomeUnknown(error) => erp_identity::Error::OutcomeUnknown(error),
        erp_audit::Error::RepositoryError(error) => erp_identity::Error::RepositoryError(error),
    }
}
