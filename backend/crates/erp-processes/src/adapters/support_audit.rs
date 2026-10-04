//! Composition adapter: processes persist support audits via erp-audit.

use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditLog, AuditLogData, prepare_business_log};
use erp_support::{PreparedSupportAudit, SupportAuditPort};
use mongodb::Database;
use persistence_core::Executor;

use crate::audit::persist_log;

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
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_error)?;
        Ok(prepared_from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedSupportAudit,
        executor: &mut dyn Executor,
    ) -> erp_support::Result<()> {
        let log = audit_log_from_prepared(audit).map_err(map_audit_error)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_error)?;
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
    .with_actor_name_snapshot(
        log.structured_event.as_ref().and_then(|event| event.actor_name_snapshot.clone()),
    )
    .with_request_id(log.structured_event.as_ref().and_then(|event| event.request_id.clone()))
    .with_event_sequence(
        log.structured_event.as_ref().map(|event| event.event_sequence).unwrap_or(NonZeroU32::MIN),
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
    prepare_business_log(&log)?
        .with_actor_name_snapshot(audit.actor_name_snapshot.clone())?
        .with_request_id(audit.request_id.clone())?
        .with_event_sequence(audit.event_sequence.get())
}

pub(crate) fn map_audit_error(error: erp_audit::Error) -> erp_support::Error {
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn prepared_audit_preserves_original_metadata_and_actor_name_snapshot() {
        for name in [Some("发生时名称".to_string()), None] {
            let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
                .with_actor_name_snapshot(name.clone())
                .unwrap();
            let domain_prepared = PreparedSupportAudit::resource(
                actor.clone(),
                "file_asset.register",
                "file_asset",
                "file-1".into(),
            )
            .unwrap();
            assert_eq!(domain_prepared.actor_name_snapshot, name);
            let mut log = actor
                .clone()
                .resource_log_with_id(
                    "audit-original".into(),
                    "file_asset.register",
                    "file_asset",
                    "file-1".into(),
                    None,
                )
                .unwrap();
            log.base = BaseModel {
                id: "audit-original".into(),
                version: 7,
                created_at: 11,
                updated_at: 19,
                deleted_at: 23,
            };
            log.structured_event.as_mut().unwrap().occurred_at = 11;
            let prepared = prepared_from_log(&log);
            let renamed = actor.with_actor_name_snapshot(Some("当前名称".into())).unwrap();
            assert_eq!(renamed.actor_name_snapshot(), Some("当前名称"));
            let restored = audit_log_from_prepared(&prepared).unwrap();
            assert_eq!(restored, log);
            assert_eq!(restored.structured_event.unwrap().actor_name_snapshot, name);
        }
    }

    #[test]
    fn prepared_audit_preserves_request_correlation_and_event_sequence() {
        for request_id in [Some("request-original".to_string()), None] {
            let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
                .with_actor_name_snapshot(Some("发生时名称".into()))
                .unwrap()
                .with_request_id(request_id.clone())
                .unwrap();
            let domain_prepared = PreparedSupportAudit::resource(
                actor.clone(),
                "file_asset.register",
                "file_asset",
                "file-1".into(),
            )
            .unwrap();
            assert_eq!(domain_prepared.request_id, request_id);
            assert_eq!(domain_prepared.event_sequence, NonZeroU32::MIN);
            let log = actor
                .clone()
                .resource_log_with_id(
                    "request-audit-original".into(),
                    "file_asset.register",
                    "file_asset",
                    "file-1".into(),
                    None,
                )
                .unwrap()
                .with_event_sequence(4)
                .unwrap();
            let prepared = prepared_from_log(&log);
            let changed = actor.with_request_id(Some("request-current".into())).unwrap();
            assert_eq!(changed.request_id(), Some("request-current"));
            let restored = audit_log_from_prepared(&prepared).unwrap();
            assert_eq!(restored, log);
            let event = restored.structured_event.as_ref().unwrap();
            assert_eq!(event.request_id, request_id);
            assert_eq!(event.event_sequence.get(), 4);
            assert_eq!(event.actor_name_snapshot.as_deref(), Some("发生时名称"));
            assert!(!restored.message.as_deref().unwrap().contains("request-original"));
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_request_correlation_and_omits_raw_input() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_request_id(Some("request-original".into()))
            .unwrap()
            .resource_log("file_asset.register", "file_asset", "file-1".into())
            .unwrap();
        let mut prepared = prepared_from_log(&log);
        prepared.message = Some("body=private-request-body;token=private-token".into());
        let restored = audit_log_from_prepared(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request-body"));
        assert!(!serialized.contains("private-token"));
        for request_id in ["request\nforged".to_string(), "r".repeat(129)] {
            prepared.request_id = Some(request_id);
            assert!(audit_log_from_prepared(&prepared).is_err());
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_names_and_drops_unprojected_message() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("发生时名称".into()))
            .unwrap()
            .resource_log("file_asset.register", "file_asset", "file-1".into())
            .unwrap();
        let mut prepared = prepared_from_log(&log);
        prepared.message = Some("token=private-request;bank=private-bank".into());
        let restored = audit_log_from_prepared(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("private-bank"));
        assert!(restored.message.as_deref().unwrap().contains("发生时名称"));
        prepared.actor_name_snapshot = Some("非法\n名称".into());
        assert!(audit_log_from_prepared(&prepared).is_err());
        prepared.actor_name_snapshot = Some("名".repeat(129));
        assert!(audit_log_from_prepared(&prepared).is_err());
        prepared.actor_id = " ".into();
        prepared.action = " ".into();
        assert!(audit_log_from_prepared(&prepared).unwrap_err().to_string().contains("操作人ID不能为空"));
    }

    #[test]
    fn audit_error_mapping_preserves_typed_transaction_sources() {
        let duplicate = map_audit_error(erp_audit::Error::ReceiptDuplicate(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            duplicate,
            erp_support::Error::ReceiptDuplicate(persistence_core::Error::OptimisticLockingError)
        ));
        let transient = map_audit_error(erp_audit::Error::TransientTransaction(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            transient,
            erp_support::Error::TransientTransaction(persistence_core::Error::OptimisticLockingError)
        ));
        let unknown = map_audit_error(erp_audit::Error::OutcomeUnknown(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            unknown,
            erp_support::Error::OutcomeUnknown(persistence_core::Error::OptimisticLockingError)
        ));
        let repository = map_audit_error(erp_audit::Error::RepositoryError(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            repository,
            erp_support::Error::RepositoryError(persistence_core::Error::OptimisticLockingError)
        ));
        assert!(
            matches!(map_audit_error(erp_audit::Error::Forbidden("original".into())), erp_support::Error::Forbidden(message) if message == "original")
        );
    }
}
