//! 仓库身份、审计与指纹 adapter。

use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditLog, AuditLogData, prepare_business_log};
use erp_core::AccountKind;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission, SharedRbacService};
use erp_support::content_fingerprint;
use erp_warehouse::{
    AttachmentFingerprintPort, HandlerIdentityFact, IdentityFactPort, PreparedWarehouseAudit,
    WarehouseAuditPort, WarehouseService,
};
use erp_workflow::entity::work_item::WorkflowAccountFact;
use erp_workflow::{AvailableWorkItemAccount, WorkItemType};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::identity_error::map_identity_error;
use crate::audit::persist_log;

map_identity_error!(erp_warehouse);

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
        Ok(prepared_warehouse_audit(&log))
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
        Ok(prepared_warehouse_audit(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedWarehouseAudit,
        executor: &mut dyn Executor,
    ) -> erp_warehouse::Result<()> {
        let log = audit_log_from_warehouse(audit).map_err(map_audit_to_warehouse)?;
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
}

impl MongoWarehouseIdentity {
    /// 绑定身份账号数据库与共享 RBAC，构造时不查询权限。
    ///
    /// # 参数
    /// * `db` - 身份账号集合所在数据库。
    /// * `rbac` - 现有 RBAC 快照服务。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 包装为仓库域可注入的身份事实 Port。
    ///
    /// # 参数
    /// * `db` - 身份账号集合所在数据库。
    /// * `rbac` - 现有 RBAC 快照服务。
    ///
    /// # 返回
    /// 返回共享的身份事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn IdentityFactPort> {
        Arc::new(Self::new(db, rbac))
    }

    async fn identity_fact(
        &self,
        account: &erp_identity::AccountCore,
    ) -> erp_warehouse::Result<HandlerIdentityFact> {
        let fact = account_fact(account);
        let can_login = AvailableWorkItemAccount::from_account(&fact).is_ok();
        let inbound = handler_permissions(required_fulfillment_permissions("purchase_receipt"));
        let outbound = handler_permissions(required_fulfillment_permissions("delivery"));
        let permissions = self
            .rbac
            .permissions(account.kind, account.base.id.as_str())
            .await
            .map_err(map_identity_error)?;
        let inbound_eligible =
            inbound.iter().all(|required| permissions.iter().any(|granted| granted.covers(required)));
        let outbound_eligible =
            outbound.iter().all(|required| permissions.iter().any(|granted| granted.covers(required)));
        Ok(HandlerIdentityFact {
            user_id: account.base.id.clone(),
            display_name: account.name.clone(),
            account: account.secret.account().to_string(),
            can_login,
            inbound_eligible,
            outbound_eligible,
        })
    }
}

#[async_trait]
impl IdentityFactPort for MongoWarehouseIdentity {
    async fn handler_identity(&self, account_id: &str) -> erp_warehouse::Result<Option<HandlerIdentityFact>> {
        let account = self
            .db
            .accounts()
            .find_work_item_account(account_id, &mut NoTransaction)
            .await
            .map_err(erp_warehouse::Error::from)?;
        match account {
            Some(account) => Ok(Some(self.identity_fact(&account).await?)),
            None => Ok(None),
        }
    }

    async fn admin_handler_identities(&self) -> erp_warehouse::Result<Vec<HandlerIdentityFact>> {
        let accounts = self
            .db
            .accounts()
            .list_by_kind(AccountKind::Admin, &mut NoTransaction)
            .await
            .map_err(erp_warehouse::Error::from)?;
        let mut facts = Vec::new();
        for account in accounts {
            facts.push(self.identity_fact(&account).await?);
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
/// * `rbac` - 现有 RBAC 快照服务。
///
/// # 返回
/// 返回仓库服务。
///
/// # 错误
/// 不返回错误。
pub fn warehouse_service(db: Database, rbac: SharedRbacService) -> WarehouseService {
    WarehouseService::new(
        db.clone(),
        MongoWarehouseIdentity::shared(db.clone(), rbac),
        MongoWarehouseAudit::shared(db),
        Arc::new(SupportFingerprint),
    )
}

fn account_fact(account: &erp_identity::AccountCore) -> WorkflowAccountFact {
    WorkflowAccountFact::new(account.base.id.clone(), account.kind, account.can_login())
        .with_display_name(account.name.clone())
        .with_login_account(account.secret.account().to_string())
}

/// 取已登记履约对象的完整执行权限。
///
/// # Panics
/// 对象类型未登记履约完整执行权限时 panic，避免静默放行。
fn required_fulfillment_permissions(business_object_type: &str) -> &'static [&'static str] {
    WorkItemType::FulfillmentOperation
        .fulfillment_execution_permissions(business_object_type)
        .expect("仓库责任对象必须登记履约完整执行权限")
}

/// 把固定仓储操作权限码解析为权限值。
///
/// # Panics
/// 固定权限码无法解析时 panic。
fn handler_permissions(codes: &[&str]) -> Vec<Permission> {
    codes.iter().map(|code| Permission::parse(code).expect("固定仓储操作权限必须合法")).collect()
}

fn prepared_warehouse_audit(log: &AuditLog) -> PreparedWarehouseAudit {
    PreparedWarehouseAudit::from_validated(
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

fn audit_log_from_warehouse(audit: &PreparedWarehouseAudit) -> erp_audit::Result<AuditLog> {
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

fn map_audit_to_warehouse(error: erp_audit::Error) -> erp_warehouse::Error {
    match error {
        erp_audit::Error::Internal(message) => erp_warehouse::Error::Internal(message),
        erp_audit::Error::NotFound(message) => erp_warehouse::Error::NotFound(message),
        erp_audit::Error::ValidationError(message) => erp_warehouse::Error::ValidationError(message),
        erp_audit::Error::BusinessLogicError(message) => erp_warehouse::Error::BusinessLogicError(message),
        erp_audit::Error::ConflictError(message) => erp_warehouse::Error::ConflictError(message),
        erp_audit::Error::ReceiptDuplicate(error) => erp_warehouse::Error::ReceiptDuplicate(error),
        erp_audit::Error::TransientTransaction(error) => erp_warehouse::Error::TransientTransaction(error),
        erp_audit::Error::Forbidden(message) => erp_warehouse::Error::Forbidden(message),
        erp_audit::Error::Unauthenticated(message) => erp_warehouse::Error::Unauthenticated(message),
        erp_audit::Error::Logic(error) => erp_warehouse::Error::Logic(error),
        erp_audit::Error::OutcomeUnknown(error) => erp_warehouse::Error::OutcomeUnknown(error),
        erp_audit::Error::RepositoryError(error) => erp_warehouse::Error::RepositoryError(error),
    }
}

#[cfg(test)]
mod tests {
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
            let prepared = prepared_warehouse_audit(&log);
            let renamed = actor.with_actor_name_snapshot(Some("当前名称".into())).unwrap();
            assert_eq!(renamed.actor_name_snapshot(), Some("当前名称"));
            let restored = audit_log_from_warehouse(&prepared).unwrap();
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
            let prepared = prepared_warehouse_audit(&log);
            let changed = actor.with_request_id(Some("request-current".into())).unwrap();
            assert_eq!(changed.request_id(), Some("request-current"));
            let restored = audit_log_from_warehouse(&prepared).unwrap();
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
        let mut prepared = prepared_warehouse_audit(&log);
        prepared.message = Some("body=private-request-body;token=private-token".into());
        let restored = audit_log_from_warehouse(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request-body"));
        assert!(!serialized.contains("private-token"));
        for request_id in ["request\nforged".to_string(), "r".repeat(129)] {
            prepared.request_id = Some(request_id);
            assert!(audit_log_from_warehouse(&prepared).is_err());
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_names_and_drops_unprojected_message() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("发生时名称".into()))
            .unwrap()
            .resource_log("warehouse.create", "warehouse", "warehouse-1".into())
            .unwrap();
        let mut prepared = prepared_warehouse_audit(&log);
        prepared.message = Some("token=private-request;bank=private-bank".into());
        let restored = audit_log_from_warehouse(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("private-bank"));
        assert!(restored.message.as_deref().unwrap().contains("发生时名称"));
        prepared.actor_name_snapshot = Some("非法\n名称".into());
        assert!(audit_log_from_warehouse(&prepared).is_err());
        prepared.actor_name_snapshot = Some("名".repeat(129));
        assert!(audit_log_from_warehouse(&prepared).is_err());
        prepared.actor_id = " ".into();
        prepared.action = " ".into();
        assert!(audit_log_from_warehouse(&prepared).unwrap_err().to_string().contains("操作人ID不能为空"));
    }
}
