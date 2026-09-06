//! Composition adapter: remaining services persist support audits via erp-audit.

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog, AuditLogData};
use erp_support::{PreparedSupportAudit, SupportAuditPort};
use mongodb::Database;
use persistence_core::Executor;

/// MongoDB adapter that converts support audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoSupportAudit {
    db: Database,
}

impl MongoSupportAudit {
    /// Bind the adapter to `db`.
    ///
    /// # Parameters
    /// * `db` - MongoDB database used to persist audit logs
    ///
    /// # Returns
    /// Adapter implementing [`SupportAuditPort`].
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn SupportAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl SupportAuditPort for MongoSupportAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_support::Result<PreparedSupportAudit> {
        let log = actor
            .resource_log(action, resource_type, resource_id)
            .map_err(map_audit_error)?;
        Ok(prepared_from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedSupportAudit,
        executor: &mut dyn Executor,
    ) -> erp_support::Result<()> {
        let log = audit_log_from_prepared(audit).map_err(map_audit_error)?;
        self.db
            .audit_logs()
            .create(&log, executor)
            .await
            .map_err(erp_support::Error::from)?;
        Ok(())
    }
}

/// Copy constructed audit fields so persist does not regenerate timestamps.
fn prepared_from_log(log: &AuditLog) -> PreparedSupportAudit {
    PreparedSupportAudit::from_validated(
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
fn audit_log_from_prepared(audit: &PreparedSupportAudit) -> erp_audit::Result<AuditLog> {
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

fn map_audit_error(error: erp_audit::Error) -> erp_support::Error {
    match error {
        erp_audit::Error::Internal(message) => erp_support::Error::Internal(message),
        erp_audit::Error::NotFound(message) => erp_support::Error::NotFound(message),
        erp_audit::Error::ValidationError(message) => erp_support::Error::ValidationError(message),
        erp_audit::Error::BusinessLogicError(message) => erp_support::Error::BusinessLogicError(message),
        erp_audit::Error::ConflictError(message) => erp_support::Error::ConflictError(message),
        erp_audit::Error::ReceiptDuplicate(error) => erp_support::Error::ReceiptDuplicate(error),
        erp_audit::Error::TransientTransaction(error) => erp_support::Error::TransientTransaction(error),
        erp_audit::Error::Forbidden(message) => erp_support::Error::Forbidden(message),
        erp_audit::Error::Unauthenticated(message) => erp_support::Error::Unauthenticated(message),
        erp_audit::Error::Logic(error) => erp_support::Error::Logic(error),
        erp_audit::Error::OutcomeUnknown(error) => erp_support::Error::OutcomeUnknown(error),
        erp_audit::Error::RepositoryError(error) => erp_support::Error::RepositoryError(error),
    }
}
