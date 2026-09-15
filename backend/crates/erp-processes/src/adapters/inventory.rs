//! Inventory authorization, audit and foreign-fact adapters.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::adapters::workflow::workflow_auth;
use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog, AuditLogData};
use erp_catalog::CatalogExt;
use erp_core::ids::SkuId;
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::{Error as IdentityError, Permission, SharedRbacService};
use erp_inventory::{
    AuthorizationPort, CatalogFactsPort, FulfillmentFactsPort, InventoryAuditPort, InventoryAuthorization,
    InventoryService, PreparedInventoryAudit, ReceiptNoFact, SkuFact, SkuRevisionFact, WarehouseFact,
    WarehouseFactsPort, WarehouseRevisionFact, WarehouseScope,
};
use erp_warehouse::WarehouseExt;
use mongodb::Database;
use persistence_core::Executor;

const DETAIL_PERMISSION: &str = "stock_adjustment:detail";
const ADJUSTMENT_LIST_PERMISSION: &str = "stock_adjustment:list";
const CREATE_PERMISSION: &str = "stock_adjustment:create";
const UPDATE_PERMISSION: &str = "stock_adjustment:update";
const BALANCE_LIST_PERMISSION: &str = "stock_balance:list";
const BALANCE_DETAIL_PERMISSION: &str = "stock_balance:detail";
const MOVEMENT_LIST_PERMISSION: &str = "stock_movement:list";
const RESERVATION_LIST_PERMISSION: &str = "stock_reservation:list";

/// MongoDB adapter that computes inventory warehouse scopes from identity facts.
#[derive(Clone)]
pub struct MongoInventoryAuthorization {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoInventoryAuthorization {
    /// Bind the adapter to `db` and shared RBAC.
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn AuthorizationPort> {
        Arc::new(Self::new(db, rbac))
    }
}

#[async_trait]
impl AuthorizationPort for MongoInventoryAuthorization {
    async fn authorize(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<InventoryAuthorization> {
        authorize_inventory(&self.db, &self.rbac, actor, executor).await
    }
}

/// Compute inventory warehouse scopes on the caller executor snapshot.
///
/// # Parameters
/// * `db` - MongoDB handle
/// * `rbac` - shared RBAC
/// * `actor` - authenticated actor
/// * `executor` - caller-chosen executor
///
/// # Errors
/// Identity, policy or data-scope lookup failures.
pub async fn authorize_inventory(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> erp_inventory::Result<InventoryAuthorization> {
    if !erp_workflow::service::approval::approval_actor_is_active_with_executor(
        &workflow_auth(db.clone(), Arc::clone(rbac)),
        actor,
        executor,
    )
    .await
    .map_err(|error| map_svc(crate::Error::from(error)))?
    {
        return Ok(InventoryAuthorization::inactive());
    }
    let service = DataScopeService::new(db.clone(), rbac.clone());
    let mut scopes = Vec::new();
    for (code, requires_detail) in [
        (BALANCE_LIST_PERMISSION, false),
        (BALANCE_DETAIL_PERMISSION, false),
        (MOVEMENT_LIST_PERMISSION, false),
        (RESERVATION_LIST_PERMISSION, false),
        (ADJUSTMENT_LIST_PERMISSION, true),
        (DETAIL_PERMISSION, false),
        (CREATE_PERMISSION, true),
        (UPDATE_PERMISSION, true),
    ] {
        scopes.push(inventory_scope(&service, actor, code, requires_detail, executor).await?);
    }
    let mut scopes = scopes.into_iter();
    let mut next = || scopes.next().expect("八项已解析范围");
    Ok(InventoryAuthorization::from_scopes(
        true,
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
        next(),
    ))
}

/// 每个库存资源动作独立解析；创建、更新及列表沿用同角色完整详情权限要求。
async fn inventory_scope(
    service: &DataScopeService,
    actor: &AuditActor,
    code: &str,
    requires_detail: bool,
    executor: &mut dyn Executor,
) -> erp_inventory::Result<WarehouseScope> {
    let (resource, action) = code.split_once(':').expect("固定库存权限合法");
    let extra = requires_detail
        .then(|| Permission::parse(DETAIL_PERMISSION).expect("固定权限合法"))
        .into_iter()
        .collect::<Vec<_>>();
    let access = match service
        .resolve_permissions(actor, resource, action, &extra, executor)
        .await
    {
        Ok(access) => access,
        Err(IdentityError::Forbidden(_)) => return Ok(WarehouseScope::empty()),
        Err(error) => return Err(map_svc(error.into())),
    };
    warehouse_scope(&access)
}

/// 仅把公共判定通过的仓库转换为库存 Port 条件；其他维度和其他资源不能补授权。
fn warehouse_scope(access: &AuthorizedDataScope) -> erp_inventory::Result<WarehouseScope> {
    let allows = |warehouse_id| {
        access.scope.allows(
            &ScopedObject {
                owned: false,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: None,
                settlement_party_id: None,
                warehouse_id,
            },
            false,
        )
    };
    if allows(None) {
        return Ok(WarehouseScope::company());
    }
    let ids = access
        .scope
        .role_clauses
        .iter()
        .chain(access.scope.user_limit.iter())
        .flat_map(|clause| clause.warehouse_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    if ids.len() > 20_000 {
        return Err(erp_inventory::Error::ValidationError(
            "库存范围超过 20000 个仓库，请缩小配置范围".into(),
        ));
    }
    Ok(WarehouseScope::from_targets(
        ids.iter()
            .filter(|id| allows(Some(id.as_str())))
            .cloned()
            .collect(),
    ))
}

/// MongoDB adapter that converts inventory audit facts into `erp-audit` writes.
#[derive(Clone)]
pub struct MongoInventoryAudit {
    db: Database,
}

impl MongoInventoryAudit {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn InventoryAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl InventoryAuditPort for MongoInventoryAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_inventory::Result<PreparedInventoryAudit> {
        let log = actor
            .resource_log(action, resource_type, resource_id)
            .map_err(map_audit_to_inventory)?;
        Ok(prepared_inventory_audit(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedInventoryAudit,
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<()> {
        let log = audit_log_from_inventory(audit).map_err(map_audit_to_inventory)?;
        self.db
            .audit_logs()
            .create(&log, executor)
            .await
            .map_err(erp_inventory::Error::from)?;
        Ok(())
    }
}

fn prepared_inventory_audit(log: &AuditLog) -> PreparedInventoryAudit {
    PreparedInventoryAudit::from_validated(
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

fn audit_log_from_inventory(audit: &PreparedInventoryAudit) -> erp_audit::Result<AuditLog> {
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

/// Warehouse identity adapter used by inventory list/detail hydration.
#[derive(Clone)]
pub struct MongoInventoryWarehouseFacts {
    db: Database,
}

impl MongoInventoryWarehouseFacts {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn WarehouseFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl WarehouseFactsPort for MongoInventoryWarehouseFacts {
    async fn warehouse_exists(&self, id: &str, executor: &mut dyn Executor) -> erp_inventory::Result<bool> {
        Ok(self
            .db
            .warehouses()
            .find_by_id(id, executor)
            .await
            .map_err(erp_inventory::Error::from)?
            .is_some())
    }

    async fn warehouses_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, WarehouseFact>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let warehouses = self
            .db
            .warehouses()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(erp_inventory::Error::from)?;
        Ok(warehouses
            .into_iter()
            .map(|warehouse| {
                (
                    warehouse.base.id.clone(),
                    WarehouseFact {
                        id: warehouse.base.id,
                        warehouse_code: warehouse.warehouse_code,
                        current_revision_id: warehouse.stable.current_revision_id,
                    },
                )
            })
            .collect())
    }

    async fn warehouse_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, WarehouseRevisionFact>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = self
            .db
            .warehouse_revisions()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(erp_inventory::Error::from)?;
        Ok(revisions
            .into_iter()
            .map(|revision| {
                (
                    revision.base.id.clone(),
                    WarehouseRevisionFact {
                        id: revision.base.id,
                        name: revision.name,
                    },
                )
            })
            .collect())
    }
}

/// SKU identity adapter used by inventory list/detail hydration.
#[derive(Clone)]
pub struct MongoInventoryCatalogFacts {
    db: Database,
}

impl MongoInventoryCatalogFacts {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn CatalogFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl CatalogFactsPort for MongoInventoryCatalogFacts {
    async fn matching_sku_ids(
        &self,
        q: &str,
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<Vec<SkuId>> {
        self.db
            .catalog()
            .inventory_sku_ids(q, executor)
            .await
            .map_err(erp_inventory::Error::from)
    }

    async fn skus_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, SkuFact>> {
        let sku_ids = ids.iter().map(|id| SkuId::new(id.clone())).collect::<Vec<_>>();
        let skus = self
            .db
            .skus()
            .find_by_ids(&sku_ids, executor)
            .await
            .map_err(erp_inventory::Error::from)?;
        Ok(skus
            .into_iter()
            .map(|sku| {
                (
                    sku.base.id.clone(),
                    SkuFact {
                        id: sku.base.id,
                        sku_no: sku.sku_no,
                        current_revision_id: sku.stable.current_revision_id,
                    },
                )
            })
            .collect())
    }

    async fn sku_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, SkuRevisionFact>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = self
            .db
            .sku_revisions()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(erp_inventory::Error::from)?;
        Ok(revisions
            .into_iter()
            .map(|revision| {
                (
                    revision.base.id.clone(),
                    SkuRevisionFact {
                        id: revision.base.id,
                        name: revision.name,
                        specification: revision.specification,
                    },
                )
            })
            .collect())
    }
}

/// Purchase-receipt number adapter used by inventory movement views.
#[derive(Clone)]
pub struct MongoInventoryFulfillmentFacts {
    db: Database,
}

impl MongoInventoryFulfillmentFacts {
    /// Bind the adapter to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Wrap the adapter as a shared port.
    pub fn shared(db: Database) -> Arc<dyn FulfillmentFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FulfillmentFactsPort for MongoInventoryFulfillmentFacts {
    async fn receipt_nos_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, ReceiptNoFact>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let receipts = self
            .db
            .purchase_receipts()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(erp_inventory::Error::from)?;
        Ok(receipts
            .into_iter()
            .map(|receipt| {
                (
                    receipt.base.id.clone(),
                    ReceiptNoFact {
                        id: receipt.base.id,
                        receipt_no: receipt.receipt_no,
                    },
                )
            })
            .collect())
    }
}

/// Construct an inventory query service with composition adapters.
pub fn inventory_service(db: Database, rbac: SharedRbacService) -> InventoryService {
    InventoryService::new(
        db.clone(),
        MongoInventoryAuthorization::shared(db.clone(), rbac),
        MongoInventoryWarehouseFacts::shared(db.clone()),
        MongoInventoryCatalogFacts::shared(db.clone()),
        MongoInventoryFulfillmentFacts::shared(db.clone()),
        MongoInventoryAudit::shared(db),
    )
}

/// Construct the inventory-adjustment process that owns cross-domain transactions.
pub fn inventory_adjustment_service(
    db: Database,
    rbac: SharedRbacService,
) -> crate::inventory_adjustment::InventoryAdjustmentService {
    crate::inventory_adjustment::InventoryAdjustmentService::new(db, rbac)
}

fn map_audit_to_inventory(error: erp_audit::Error) -> erp_inventory::Error {
    map_svc(crate::Error::from(error))
}

fn map_svc(error: crate::Error) -> erp_inventory::Error {
    match error {
        crate::Error::Internal(message) => erp_inventory::Error::Internal(message),
        crate::Error::NotFound(message) => erp_inventory::Error::NotFound(message),
        crate::Error::ValidationError(message) => erp_inventory::Error::ValidationError(message),
        crate::Error::BusinessLogicError(message) => erp_inventory::Error::BusinessLogicError(message),
        crate::Error::ConflictError(message) => erp_inventory::Error::ConflictError(message),
        crate::Error::ReceiptDuplicate(error) => erp_inventory::Error::ReceiptDuplicate(error),
        crate::Error::TransientTransaction(error) => erp_inventory::Error::TransientTransaction(error),
        crate::Error::Forbidden(message) => erp_inventory::Error::Forbidden(message),
        crate::Error::Unauthenticated(message) => erp_inventory::Error::Unauthenticated(message),
        crate::Error::Logic(error) => erp_inventory::Error::Logic(error),
        crate::Error::OutcomeUnknown(error) => erp_inventory::Error::OutcomeUnknown(error),
        crate::Error::RepositoryError(error) => erp_inventory::Error::RepositoryError(error),
        other => erp_inventory::Error::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erp_core::common::time::Instant;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};

    #[test]
    fn inventory_scope_keeps_dimension_and_user_limit_without_fallback() {
        let mut access = AuthorizedDataScope {
            user_id: "warehouse".into(),
            resource: "stock_balance".into(),
            action: "list".into(),
            role_scopes: Default::default(),
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "1".into(),
            as_of: Instant::from_unix_secs(1),
            scope: ResolvedScope {
                role_clauses: vec![],
                user_limit: None,
            },
        };
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::empty());
        access.scope.role_clauses.push(ScopeClause {
            org_unit_ids: BTreeSet::from(["same-id".into()]),
            ..Default::default()
        });
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::empty());
        access.scope.role_clauses.push(ScopeClause {
            company: true,
            ..Default::default()
        });
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::company());
        access.scope.user_limit = Some(ScopeClause {
            warehouse_ids: BTreeSet::from(["warehouse-a".into()]),
            ..Default::default()
        });
        assert_eq!(
            warehouse_scope(&access).unwrap(),
            WarehouseScope::from_targets(vec!["warehouse-a".into()])
        );
        access.scope.user_limit = Some(ScopeClause::default());
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::empty());
    }
}
