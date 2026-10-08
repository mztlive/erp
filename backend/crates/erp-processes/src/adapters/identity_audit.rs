//! 流程与入口经 `erp-audit` 持久化身份审计。

use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditLog, AuditLogData, prepare_business_log};
use erp_identity::{IdentityAuditPort, PreparedResourceAudit};
use mongodb::Database;
use persistence_core::Executor;

use crate::audit::persist_log;

/// 把身份审计事实写入 `erp-audit` 的 Mongo adapter。
#[derive(Clone)]
pub struct MongoIdentityAudit {
    db: Database,
}

impl MongoIdentityAudit {
    /// 绑定审计日志所在数据库，构造时不写库。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter，而不是共享 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为身份域可注入的共享审计 Port。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回实现 `IdentityAuditPort` 的共享 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_error)?;
        Ok(prepared_from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedResourceAudit,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<()> {
        let log = audit_log_from_prepared(audit).map_err(map_audit_error)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_error)?;
        Ok(())
    }
}

/// 拷贝已构造的审计字段，避免持久化时重新生成时间戳。
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
    .with_actor_name_snapshot(
        log.structured_event.as_ref().and_then(|event| event.actor_name_snapshot.clone()),
    )
    .with_request_id(log.structured_event.as_ref().and_then(|event| event.request_id.clone()))
    .with_event_sequence(
        log.structured_event.as_ref().map(|event| event.event_sequence).unwrap_or(NonZeroU32::MIN),
    )
}

/// 用原来的 `BaseModel` 快照重建审计实体。
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
    prepare_business_log(&log)?
        .with_actor_name_snapshot(audit.actor_name_snapshot.clone())?
        .with_request_id(audit.request_id.clone())?
        .with_event_sequence(audit.event_sequence.get())
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn prepared_audit_preserves_frozen_name_original_metadata_and_safe_business_fields() {
        let mut log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("周晓彤".into()))
            .unwrap()
            .with_request_id(Some("request-original".into()))
            .unwrap()
            .resource_log("admin.update", "admin", "object-1".into())
            .unwrap()
            .with_event_sequence(7)
            .unwrap();
        log.base = BaseModel {
            id: "audit-original".into(),
            version: 7,
            created_at: 11,
            updated_at: 19,
            deleted_at: 23,
        };
        log.structured_event.as_mut().unwrap().occurred_at = log.base.created_at;
        let log = prepare_business_log(&log).unwrap();
        let prepared = prepared_from_log(&log);
        assert_eq!(prepared.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(prepared.request_id.as_deref(), Some("request-original"));
        assert_eq!(prepared.event_sequence.get(), 7);
        let renamed_actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("新名称".into()))
            .unwrap()
            .with_request_id(Some("request-next".into()))
            .unwrap();
        assert_eq!(renamed_actor.actor_name_snapshot(), Some("新名称"));
        assert_eq!(renamed_actor.request_id(), Some("request-next"));
        let restored = audit_log_from_prepared(&prepared).unwrap();
        assert_eq!(restored, log);
        assert!(restored.message.as_deref().unwrap().contains("周晓彤"));
        let mut invalid = prepared;
        invalid.actor_id = " ".into();
        invalid.action = " ".into();
        assert!(audit_log_from_prepared(&invalid).unwrap_err().to_string().contains("操作人ID不能为空"));
    }

    #[test]
    fn prepared_audit_unknown_name_and_private_body_remain_outside_safe_event() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .resource_log("admin.update", "admin", "object-1".into())
            .unwrap();
        let mut prepared = prepared_from_log(&log);
        prepared.message = Some("password bank-account ciphertext private-request".into());
        let restored = audit_log_from_prepared(&prepared).unwrap();
        let event = restored.structured_event.as_ref().unwrap();
        assert_eq!(event.actor_name_snapshot, None);
        assert_eq!(event.request_id, None);
        assert_eq!(event.event_sequence, NonZeroU32::MIN);
        assert_eq!(event.actor_account, "login");
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("bank-account"));
        assert!(!serialized.contains("ciphertext"));
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("actor_name_snapshot"));
        assert!(!serialized.contains("request_id"));
        assert!(restored.message.as_deref().unwrap().contains("login"));
    }

    #[test]
    fn prepared_audit_rejects_invalid_request_id() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .resource_log("admin.update", "admin", "object-1".into())
            .unwrap();
        for request_id in ["unsafe\nrequest".into(), "x".repeat(129)] {
            let prepared = prepared_from_log(&log).with_request_id(Some(request_id));
            assert!(audit_log_from_prepared(&prepared).unwrap_err().to_string().contains("请求编号"));
        }
    }

    #[test]
    fn prepared_audit_rejects_invalid_name_snapshot() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .resource_log("admin.update", "admin", "object-1".into())
            .unwrap();
        let prepared = prepared_from_log(&log).with_actor_name_snapshot(Some("unsafe\nname".into()));
        assert!(audit_log_from_prepared(&prepared).is_err());
    }

    #[test]
    fn audit_error_mapping_preserves_typed_transaction_sources() {
        let duplicate = map_audit_error(erp_audit::Error::ReceiptDuplicate(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            duplicate,
            erp_identity::Error::ReceiptDuplicate(persistence_core::Error::OptimisticLockingError)
        ));
        let transient = map_audit_error(erp_audit::Error::TransientTransaction(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            transient,
            erp_identity::Error::TransientTransaction(persistence_core::Error::OptimisticLockingError)
        ));
        let unknown = map_audit_error(erp_audit::Error::OutcomeUnknown(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            unknown,
            erp_identity::Error::OutcomeUnknown(persistence_core::Error::OptimisticLockingError)
        ));
        let repository = map_audit_error(erp_audit::Error::RepositoryError(
            persistence_core::Error::OptimisticLockingError,
        ));
        assert!(matches!(
            repository,
            erp_identity::Error::RepositoryError(persistence_core::Error::OptimisticLockingError)
        ));
        assert!(
            matches!(map_audit_error(erp_audit::Error::Forbidden("original".into())), erp_identity::Error::Forbidden(message) if message == "original")
        );
    }
}
