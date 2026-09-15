//! Party audit and supplier-role adapters.

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog, AuditLogData};
use erp_core::ids::PartyId;
use erp_party::{PartyAuditPort, PreparedPartyAudit, SupplierRolePort};
use erp_supplier::SupplierExt;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

/// MongoDB adapter that converts party audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoPartyAudit {
    db: Database,
}

impl MongoPartyAudit {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn PartyAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl PartyAuditPort for MongoPartyAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_party::Result<PreparedPartyAudit> {
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_to_party)?;
        Ok(prepared_party_audit(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedPartyAudit,
        executor: &mut dyn Executor,
    ) -> erp_party::Result<()> {
        let log = audit_log_from_party(audit).map_err(map_audit_to_party)?;
        self.db.audit_logs().create(&log, executor).await.map_err(erp_party::Error::from)?;
        Ok(())
    }
}

/// MongoDB adapter that reads whether a party currently has a supplier role.
#[derive(Clone)]
pub struct MongoSupplierRole {
    db: Database,
}

impl MongoSupplierRole {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn SupplierRolePort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl SupplierRolePort for MongoSupplierRole {
    async fn party_has_supplier_role(&self, party_id: &PartyId) -> erp_party::Result<bool> {
        Ok(self
            .db
            .supplier_accounts()
            .find_by_party(party_id, &mut NoTransaction)
            .await
            .map_err(erp_party::Error::from)?
            .is_some())
    }
}

fn prepared_party_audit(log: &AuditLog) -> PreparedPartyAudit {
    PreparedPartyAudit::from_validated(
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

fn audit_log_from_party(audit: &PreparedPartyAudit) -> erp_audit::Result<AuditLog> {
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

fn map_audit_to_party(error: erp_audit::Error) -> erp_party::Error {
    match error {
        erp_audit::Error::Internal(message) => erp_party::Error::Internal(message),
        erp_audit::Error::NotFound(message) => erp_party::Error::NotFound(message),
        erp_audit::Error::ValidationError(message) => erp_party::Error::ValidationError(message),
        erp_audit::Error::BusinessLogicError(message) => erp_party::Error::BusinessLogicError(message),
        erp_audit::Error::ConflictError(message) => erp_party::Error::ConflictError(message),
        erp_audit::Error::ReceiptDuplicate(error) => erp_party::Error::ReceiptDuplicate(error),
        erp_audit::Error::TransientTransaction(error) => erp_party::Error::TransientTransaction(error),
        erp_audit::Error::Forbidden(message) => erp_party::Error::Forbidden(message),
        erp_audit::Error::Unauthenticated(message) => erp_party::Error::Unauthenticated(message),
        erp_audit::Error::Logic(error) => erp_party::Error::Logic(error),
        erp_audit::Error::OutcomeUnknown(error) => erp_party::Error::OutcomeUnknown(error),
        erp_audit::Error::RepositoryError(error) => erp_party::Error::RepositoryError(error),
    }
}
