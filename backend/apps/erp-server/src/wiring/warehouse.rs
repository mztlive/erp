//! 仓库身份、审计与指纹 adapter。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::AccountKind;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::repository::prelude::*;
use erp_identity::{
    AccessControlExt, AccessTarget, AccountCore, AuthorizationService, Error as IdentityError,
    SharedRbacService,
};
use erp_permissions::internal;
use erp_support::content_fingerprint;
use erp_warehouse::{
    AttachmentFingerprintPort, HandlerIdentityFact, IdentityFactPort, PreparedWarehouseAudit,
    WarehouseAuditPort, WarehouseService,
};
use mongodb::Database;
use persistence_core::Executor;

use super::audit_conversion::{PreparedAudit, map_audit_error};

/// 把仓库审计事实写入 `erp-audit` 的 Mongo adapter。
#[derive(Clone)]
pub struct MongoWarehouseAudit {
    db: Database,
}

impl MongoWarehouseAudit {
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

    /// 包装为仓库域可注入的共享审计 Port。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回共享的仓库审计 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn WarehouseAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl WarehouseAuditPort for MongoWarehouseAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_warehouse::Result<PreparedWarehouseAudit> {
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_to_warehouse)?;
        Ok(PreparedWarehouseAudit::from_log(&log))
    }

    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> erp_warehouse::Result<PreparedWarehouseAudit> {
        let log = actor
            .resource_log_with_message(action, resource_type, resource_id, message)
            .map_err(map_audit_to_warehouse)?;
        Ok(PreparedWarehouseAudit::from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedWarehouseAudit,
        executor: &mut dyn Executor,
    ) -> erp_warehouse::Result<()> {
        let log = audit.to_log().map_err(map_audit_to_warehouse)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_warehouse)?;
        Ok(())
    }
}

/// 判断入库与出库处理人资格的身份 adapter。
///
/// 入库覆盖 `purchase_receipt:list/detail/update/post`；出库覆盖
/// `delivery:list/detail/update/post`。
#[derive(Clone)]
pub struct MongoWarehouseIdentity {
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
}

impl MongoWarehouseIdentity {
    /// 绑定身份账号数据库与共享 RBAC，构造时不查询权限。
    ///
    /// # 参数
    /// * `db` - 身份账号集合所在数据库。
    /// * `rbac` - 现有账号身份设施。
    /// * `authorization` - 进程共享的完整授权服务。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, rbac, authorization }
    }

    /// 包装为仓库域可注入的身份事实 Port。
    ///
    /// # 参数
    /// * `db` - 身份账号集合所在数据库。
    /// * `rbac` - 现有账号身份设施。
    /// * `authorization` - 进程共享的完整授权服务。
    ///
    /// # 返回
    /// 返回共享的身份事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(
        db: Database,
        rbac: SharedRbacService,
        authorization: Arc<AuthorizationService>,
    ) -> Arc<dyn IdentityFactPort> {
        Arc::new(Self::new(db, rbac, authorization))
    }

    async fn identity_fact(
        &self,
        account: &AccountCore,
        executor: &mut dyn Executor,
    ) -> erp_warehouse::Result<HandlerIdentityFact> {
        let actor =
            AuditActor::new(account.base.id.clone(), account.secret.account().to_string(), account.kind);
        let can_login = self.rbac.qualified_permissions(&actor, &[], executor).await?.is_some();
        let inbound_eligible = self.execution_qualified(&actor, "purchase_receipt", executor).await?;
        let outbound_eligible = self.execution_qualified(&actor, "delivery", executor).await?;
        Ok(HandlerIdentityFact {
            user_id: account.base.id.clone(),
            display_name: account.name.clone(),
            account: account.secret.account().to_string(),
            can_login,
            inbound_eligible,
            outbound_eligible,
        })
    }

    async fn execution_qualified(
        &self,
        actor: &AuditActor,
        source: &str,
        executor: &mut dyn Executor,
    ) -> erp_warehouse::Result<bool> {
        let target = AccessTarget::Candidate(ObjectFacts {
            resource: "work_item".into(),
            subject_type: FactValue::Known("FULFILLMENT_OPERATION".into()),
            subject_source_resource: FactValue::Known(source.into()),
            ..Default::default()
        });
        match self
            .authorization
            .require(actor, &internal::WORK_ITEM_EXECUTION_QUALIFICATION, target, executor)
            .await
        {
            Ok(()) => Ok(true),
            Err(IdentityError::Forbidden(_)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

#[async_trait]
impl IdentityFactPort for MongoWarehouseIdentity {
    async fn handler_identity(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_warehouse::Result<Option<HandlerIdentityFact>> {
        let account = self
            .db
            .accounts()
            .find_work_item_account(account_id, executor)
            .await
            .map_err(erp_warehouse::Error::from)?;
        match account {
            Some(account) => Ok(Some(self.identity_fact(&account, executor).await?)),
            None => Ok(None),
        }
    }

    async fn admin_handler_identities(
        &self,
        executor: &mut dyn Executor,
    ) -> erp_warehouse::Result<Vec<HandlerIdentityFact>> {
        let accounts = self
            .db
            .accounts()
            .list_by_kind(AccountKind::Admin, executor)
            .await
            .map_err(erp_warehouse::Error::from)?;
        let mut facts = Vec::new();
        for account in accounts {
            facts.push(self.identity_fact(&account, executor).await?);
        }
        Ok(facts)
    }
}

/// 委托支持域唯一 HMAC 实现的附件指纹 adapter。
#[derive(Debug, Default, Clone, Copy)]
pub struct SupportFingerprint;

impl AttachmentFingerprintPort for SupportFingerprint {
    fn content_fingerprint(&self, plain: &str, key: &[u8]) -> String {
        content_fingerprint(plain, key)
    }
}

/// 装配带身份、审计与指纹 adapter 的仓库服务。
///
/// # 参数
/// * `db` - 仓库与身份集合所在数据库。
/// * `rbac` - 现有账号身份设施。
/// * `authorization` - 进程共享的完整授权服务。
///
/// # 返回
/// 返回仓库服务。
///
/// # 错误
/// 不返回错误。
pub fn warehouse_service(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> WarehouseService {
    WarehouseService::new(
        db.clone(),
        MongoWarehouseIdentity::shared(db.clone(), rbac, authorization.clone()),
        MongoWarehouseAudit::shared(db),
        Arc::new(SupportFingerprint),
        authorization,
    )
}

map_audit_error!(fn map_audit_to_warehouse -> erp_warehouse);

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use entity_core::BaseModel;

    use super::*;

    #[test]
    fn prepared_audit_preserves_original_metadata_and_actor_name_snapshot() {
        for name in [Some("发生时名称".to_string()), None] {
            let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
                .with_actor_name_snapshot(name.clone())
                .unwrap();
            let domain_prepared = PreparedWarehouseAudit::resource(
                actor.clone(),
                "warehouse.create",
                "warehouse",
                "warehouse-1".into(),
            )
            .unwrap();
            assert_eq!(domain_prepared.actor_name_snapshot, name);
            let mut log = actor
                .clone()
                .resource_log_with_id(
                    "audit-original".into(),
                    "warehouse.create",
                    "warehouse",
                    "warehouse-1".into(),
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
            let prepared = PreparedWarehouseAudit::from_log(&log);
            let renamed = actor.with_actor_name_snapshot(Some("当前名称".into())).unwrap();
            assert_eq!(renamed.actor_name_snapshot(), Some("当前名称"));
            let restored = prepared.to_log().unwrap();
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
            let domain_prepared = PreparedWarehouseAudit::resource(
                actor.clone(),
                "warehouse.create",
                "warehouse",
                "warehouse-1".into(),
            )
            .unwrap();
            assert_eq!(domain_prepared.request_id, request_id);
            assert_eq!(domain_prepared.event_sequence, NonZeroU32::MIN);
            let log = actor
                .clone()
                .resource_log_with_id(
                    "request-audit-original".into(),
                    "warehouse.create",
                    "warehouse",
                    "warehouse-1".into(),
                    None,
                )
                .unwrap()
                .with_event_sequence(4)
                .unwrap();
            let prepared = PreparedWarehouseAudit::from_log(&log);
            let changed = actor.with_request_id(Some("request-current".into())).unwrap();
            assert_eq!(changed.request_id(), Some("request-current"));
            let restored = prepared.to_log().unwrap();
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
            .resource_log("warehouse.create", "warehouse", "warehouse-1".into())
            .unwrap();
        let mut prepared = PreparedWarehouseAudit::from_log(&log);
        prepared.message = Some("body=private-request-body;token=private-token".into());
        let restored = prepared.to_log().unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request-body"));
        assert!(!serialized.contains("private-token"));
        for request_id in ["request\nforged".to_string(), "r".repeat(129)] {
            prepared.request_id = Some(request_id);
            assert!(prepared.to_log().is_err());
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_names_and_drops_unprojected_message() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("发生时名称".into()))
            .unwrap()
            .resource_log("warehouse.create", "warehouse", "warehouse-1".into())
            .unwrap();
        let mut prepared = PreparedWarehouseAudit::from_log(&log);
        prepared.message = Some("token=private-request;bank=private-bank".into());
        let restored = prepared.to_log().unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("private-bank"));
        assert!(restored.message.as_deref().unwrap().contains("发生时名称"));
        prepared.actor_name_snapshot = Some("非法\n名称".into());
        assert!(prepared.to_log().is_err());
        prepared.actor_name_snapshot = Some("名".repeat(129));
        assert!(prepared.to_log().is_err());
        prepared.actor_id = " ".into();
        prepared.action = " ".into();
        assert!(prepared.to_log().unwrap_err().to_string().contains("操作人ID不能为空"));
    }
}
