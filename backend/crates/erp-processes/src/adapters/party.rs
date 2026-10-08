//! 主体审计与供应商角色 adapter。

use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditLog, AuditLogData, prepare_business_log};
use erp_core::ids::PartyId;
use erp_party::{PartyAuditPort, PreparedPartyAudit, SupplierRolePort};
use erp_supplier::SupplierExt;
use erp_supplier::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::audit::persist_log;

/// 把主体审计事实写入 `erp-audit` 的 Mongo adapter。
#[derive(Clone)]
pub struct MongoPartyAudit {
    db: Database,
}

impl MongoPartyAudit {
    /// 绑定审计日志所在数据库，构造时不写库。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为主体域可注入的共享审计 Port。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回共享的主体审计 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_party)?;
        Ok(())
    }
}

/// 读取主体当前是否具有供应商角色的 Mongo adapter。
#[derive(Clone)]
pub struct MongoSupplierRole {
    db: Database,
}

impl MongoSupplierRole {
    /// 绑定供应商账号集合所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 供应商账号所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为主体域可注入的供应商角色 Port。
    ///
    /// # 参数
    /// * `db` - 供应商账号所在数据库。
    ///
    /// # 返回
    /// 返回共享的供应商角色 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
    .with_actor_name_snapshot(
        log.structured_event.as_ref().and_then(|event| event.actor_name_snapshot.clone()),
    )
    .with_request_id(log.structured_event.as_ref().and_then(|event| event.request_id.clone()))
    .with_event_sequence(
        log.structured_event.as_ref().map(|event| event.event_sequence).unwrap_or(NonZeroU32::MIN),
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
    prepare_business_log(&log)?
        .with_actor_name_snapshot(audit.actor_name_snapshot.clone())?
        .with_request_id(audit.request_id.clone())?
        .with_event_sequence(audit.event_sequence.get())
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
    }

    #[test]
    fn prepared_audit_round_trip_freezes_name_and_original_event_metadata() {
        let mut log = actor()
            .with_actor_name_snapshot(Some("周晓彤".into()))
            .unwrap()
            .with_request_id(Some("request-original".into()))
            .unwrap()
            .resource_log("party.update", "party", "object-1".into())
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
        let prepared = prepared_party_audit(&log);
        assert_eq!(prepared.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(prepared.request_id.as_deref(), Some("request-original"));
        assert_eq!(prepared.event_sequence.get(), 7);
        let renamed_actor = actor()
            .with_actor_name_snapshot(Some("新名称".into()))
            .unwrap()
            .with_request_id(Some("request-next".into()))
            .unwrap();
        assert_eq!(renamed_actor.actor_name_snapshot(), Some("新名称"));
        assert_eq!(renamed_actor.request_id(), Some("request-next"));
        let restored = audit_log_from_party(&prepared).unwrap();
        assert_eq!(restored, log);
        assert!(restored.message.as_deref().unwrap().contains("周晓彤"));
        assert!(restored.message.as_deref().unwrap().contains("主体"));
    }

    #[test]
    fn prepared_audit_preserves_unknown_name_and_discards_private_body() {
        let log = actor().resource_log("party.update", "party", "object-1".into()).unwrap();
        let mut prepared = prepared_party_audit(&log);
        prepared.message = Some("password bank-account ciphertext private-request".into());
        let restored = audit_log_from_party(&prepared).unwrap();
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
        let log = actor().resource_log("party.update", "party", "object-1".into()).unwrap();
        for request_id in ["unsafe\nrequest".into(), "x".repeat(129)] {
            let prepared = prepared_party_audit(&log).with_request_id(Some(request_id));
            assert!(audit_log_from_party(&prepared).unwrap_err().to_string().contains("请求编号"));
        }
    }

    #[test]
    fn prepared_audit_rejects_invalid_name_snapshot() {
        let log = actor().resource_log("party.update", "party", "object-1".into()).unwrap();
        let prepared = prepared_party_audit(&log).with_actor_name_snapshot(Some("unsafe\nname".into()));
        assert!(audit_log_from_party(&prepared).is_err());
    }
}
