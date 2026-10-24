//! 主体审计与供应商角色 adapter。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_core::ids::PartyId;
use erp_party::{PartyAuditPort, PreparedPartyAudit, SupplierRolePort};
use erp_supplier::SupplierExt;
use erp_supplier::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::audit_conversion::{PreparedAudit, map_audit_error};
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
        Ok(PreparedPartyAudit::from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedPartyAudit,
        executor: &mut dyn Executor,
    ) -> erp_party::Result<()> {
        let log = audit.to_log().map_err(map_audit_to_party)?;
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

map_audit_error!(fn map_audit_to_party -> erp_party);

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use entity_core::BaseModel;
    use erp_audit::prepare_business_log;
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
        let prepared = PreparedPartyAudit::from_log(&log);
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
        let restored = prepared.to_log().unwrap();
        assert_eq!(restored, log);
        assert!(restored.message.as_deref().unwrap().contains("周晓彤"));
        assert!(restored.message.as_deref().unwrap().contains("主体"));
    }

    #[test]
    fn prepared_audit_preserves_unknown_name_and_discards_private_body() {
        let log = actor().resource_log("party.update", "party", "object-1".into()).unwrap();
        let mut prepared = PreparedPartyAudit::from_log(&log);
        prepared.message = Some("password bank-account ciphertext private-request".into());
        let restored = prepared.to_log().unwrap();
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
            let prepared = PreparedPartyAudit::from_log(&log).with_request_id(Some(request_id));
            assert!(prepared.to_log().unwrap_err().to_string().contains("请求编号"));
        }
    }

    #[test]
    fn prepared_audit_rejects_invalid_name_snapshot() {
        let log = actor().resource_log("party.update", "party", "object-1".into()).unwrap();
        let prepared =
            PreparedPartyAudit::from_log(&log).with_actor_name_snapshot(Some("unsafe\nname".into()));
        assert!(prepared.to_log().is_err());
    }
}
