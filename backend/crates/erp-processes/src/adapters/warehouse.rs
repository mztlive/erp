//! Warehouse identity, audit and fingerprint adapters.

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog, AuditLogData};
use erp_core::AccountKind;
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

/// MongoDB adapter that converts warehouse audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoWarehouseAudit {
    db: Database,
}

impl MongoWarehouseAudit {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
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
        self.db.audit_logs().create(&log, executor).await.map_err(erp_warehouse::Error::from)?;
        Ok(())
    }
}

/// Identity adapter that evaluates inbound/outbound warehouse handler eligibility.
///
/// Inbound covers `purchase_receipt:list/detail/update/post`; outbound covers
/// `delivery:list/detail/update/post`.
#[derive(Clone)]
pub struct MongoWarehouseIdentity {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoWarehouseIdentity {
    /// Bind the adapter to `db` and the shared RBAC service.
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// Wrap the adapter as a shared port.
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
            .map_err(map_identity_to_warehouse)?;
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

/// Fingerprint adapter that delegates to the unique support HMAC implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct SupportFingerprint;

impl AttachmentFingerprintPort for SupportFingerprint {
    fn content_fingerprint(&self, plain: &str, key: &[u8]) -> String {
        content_fingerprint(plain, key)
    }
}

/// Construct a warehouse service with identity, audit and fingerprint adapters.
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

fn required_fulfillment_permissions(business_object_type: &str) -> &'static [&'static str] {
    WorkItemType::FulfillmentOperation
        .fulfillment_execution_permissions(business_object_type)
        .expect("仓库责任对象必须登记履约完整执行权限")
}

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
    Ok(log)
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

fn map_identity_to_warehouse(error: erp_identity::Error) -> erp_warehouse::Error {
    match error {
        erp_identity::Error::Internal(message) => erp_warehouse::Error::Internal(message),
        erp_identity::Error::NotFound(message) => erp_warehouse::Error::NotFound(message),
        erp_identity::Error::ValidationError(message) => erp_warehouse::Error::ValidationError(message),
        erp_identity::Error::BusinessLogicError(message) => erp_warehouse::Error::BusinessLogicError(message),
        erp_identity::Error::ConflictError(message) => erp_warehouse::Error::ConflictError(message),
        erp_identity::Error::ReceiptDuplicate(error) => erp_warehouse::Error::ReceiptDuplicate(error),
        erp_identity::Error::TransientTransaction(error) => erp_warehouse::Error::TransientTransaction(error),
        erp_identity::Error::Forbidden(message) => erp_warehouse::Error::Forbidden(message),
        erp_identity::Error::Unauthenticated(message) => erp_warehouse::Error::Unauthenticated(message),
        erp_identity::Error::Logic(error) => erp_warehouse::Error::Logic(error),
        erp_identity::Error::Rbac(message) => erp_warehouse::Error::Internal(message),
        erp_identity::Error::OutcomeUnknown(error) => erp_warehouse::Error::OutcomeUnknown(error),
        erp_identity::Error::RepositoryError(error) => erp_warehouse::Error::RepositoryError(error),
    }
}
