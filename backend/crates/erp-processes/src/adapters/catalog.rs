//! Catalog audit, file-asset and pending-attachment adapters.

use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog, AuditLogData};
use erp_catalog::{
    CatalogAuditPort, CatalogService, FileAssetFact, FileAssetFactsPort, PendingAttachmentBatch,
    PreparedCatalogAudit,
};
use erp_core::ids::FileAssetId;
use erp_support::FileAssetExt;
use mongodb::Database;
use persistence_core::Executor;

/// MongoDB adapter that converts catalog audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoCatalogAudit {
    db: Database,
}

impl MongoCatalogAudit {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn CatalogAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl CatalogAuditPort for MongoCatalogAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_catalog::Result<PreparedCatalogAudit> {
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_to_catalog)?;
        Ok(prepared_catalog_audit(&log))
    }

    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> erp_catalog::Result<PreparedCatalogAudit> {
        let log = actor
            .resource_log_with_message(action, resource_type, resource_id, message)
            .map_err(map_audit_to_catalog)?;
        Ok(prepared_catalog_audit(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedCatalogAudit,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<()> {
        let log = audit_log_from_catalog(audit).map_err(map_audit_to_catalog)?;
        self.db.audit_logs().create(&log, executor).await.map_err(erp_catalog::Error::from)?;
        Ok(())
    }
}

/// MongoDB adapter that reads file-asset existence for catalog media and logos.
#[derive(Clone)]
pub struct MongoCatalogFileAssets {
    db: Database,
}

impl MongoCatalogFileAssets {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn FileAssetFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FileAssetFactsPort for MongoCatalogFileAssets {
    async fn find_by_id(
        &self,
        asset_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<Option<FileAssetFact>> {
        Ok(self
            .db
            .file_assets()
            .find_by_id(asset_id.as_ref(), executor)
            .await
            .map_err(erp_catalog::Error::from)?
            .map(|asset| FileAssetFact { id: asset.base.id }))
    }

    async fn missing_ids(
        &self,
        asset_ids: &[FileAssetId],
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<Vec<FileAssetId>> {
        self.db
            .file_assets()
            .missing_file_asset_ids(asset_ids, executor)
            .await
            .map_err(erp_catalog::Error::from)
    }
}

/// Adapter that exposes a prepared support attachment batch through catalog's port.
pub struct CatalogPendingAttachments {
    inner: Arc<dyn erp_support::PendingAttachmentBatch>,
}

impl CatalogPendingAttachments {
    /// Wrap a support pending batch as catalog's in-transaction pending port.
    pub fn from_support(
        inner: Arc<dyn erp_support::PendingAttachmentBatch>,
    ) -> Arc<dyn PendingAttachmentBatch> {
        Arc::new(Self { inner })
    }
}

#[async_trait]
impl PendingAttachmentBatch for CatalogPendingAttachments {
    fn resolve_id(&self, id: &mut FileAssetId, used: &mut HashSet<String>) -> erp_core::Result<bool> {
        self.inner.resolve_id(id, used)
    }

    fn ensure_all_used(&self, used: &HashSet<String>) -> erp_core::Result<()> {
        self.inner.ensure_all_used(used)
    }

    fn contains_id(&self, id: &FileAssetId) -> bool {
        self.inner.contains_id(id)
    }

    async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> erp_catalog::Result<()> {
        self.inner.persist(db, executor).await.map_err(map_support_to_catalog)
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Construct a catalog service with audit and file-asset adapters.
pub fn catalog_service(db: Database) -> CatalogService {
    CatalogService::new(db.clone(), MongoCatalogAudit::shared(db.clone()), MongoCatalogFileAssets::shared(db))
}

fn prepared_catalog_audit(log: &AuditLog) -> PreparedCatalogAudit {
    PreparedCatalogAudit::from_validated(
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

fn audit_log_from_catalog(audit: &PreparedCatalogAudit) -> erp_audit::Result<AuditLog> {
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

fn map_audit_to_catalog(error: erp_audit::Error) -> erp_catalog::Error {
    match error {
        erp_audit::Error::Internal(message) => erp_catalog::Error::Internal(message),
        erp_audit::Error::NotFound(message) => erp_catalog::Error::NotFound(message),
        erp_audit::Error::ValidationError(message) => erp_catalog::Error::ValidationError(message),
        erp_audit::Error::BusinessLogicError(message) => erp_catalog::Error::BusinessLogicError(message),
        erp_audit::Error::ConflictError(message) => erp_catalog::Error::ConflictError(message),
        erp_audit::Error::ReceiptDuplicate(error) => erp_catalog::Error::ReceiptDuplicate(error),
        erp_audit::Error::TransientTransaction(error) => erp_catalog::Error::TransientTransaction(error),
        erp_audit::Error::Forbidden(message) => erp_catalog::Error::Forbidden(message),
        erp_audit::Error::Unauthenticated(message) => erp_catalog::Error::Unauthenticated(message),
        erp_audit::Error::Logic(error) => erp_catalog::Error::Logic(error),
        erp_audit::Error::OutcomeUnknown(error) => erp_catalog::Error::OutcomeUnknown(error),
        erp_audit::Error::RepositoryError(error) => erp_catalog::Error::RepositoryError(error),
    }
}

fn map_support_to_catalog(error: erp_support::Error) -> erp_catalog::Error {
    match error {
        erp_support::Error::Internal(message) => erp_catalog::Error::Internal(message),
        erp_support::Error::NotFound(message) => erp_catalog::Error::NotFound(message),
        erp_support::Error::ValidationError(message) => erp_catalog::Error::ValidationError(message),
        erp_support::Error::BusinessLogicError(message) => erp_catalog::Error::BusinessLogicError(message),
        erp_support::Error::ConflictError(message) => erp_catalog::Error::ConflictError(message),
        erp_support::Error::ReceiptDuplicate(error) => erp_catalog::Error::ReceiptDuplicate(error),
        erp_support::Error::TransientTransaction(error) => erp_catalog::Error::TransientTransaction(error),
        erp_support::Error::Forbidden(message) => erp_catalog::Error::Forbidden(message),
        erp_support::Error::Unauthenticated(message) => erp_catalog::Error::Unauthenticated(message),
        erp_support::Error::Logic(error) => erp_catalog::Error::Logic(error),
        erp_support::Error::OutcomeUnknown(error) => erp_catalog::Error::OutcomeUnknown(error),
        erp_support::Error::RepositoryError(error) => erp_catalog::Error::RepositoryError(error),
    }
}
