//! Inventory authorization, audit and foreign-fact adapters.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::adapters::workflow::workflow_auth;
use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditExt, AuditLog, AuditLogData};
use erp_catalog::CatalogExt;
use erp_core::ids::SkuId;
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::access_control::{DataScope, DataScopeSubjectType, OrganizationCoverage};
use erp_identity::{AccessControlExt, Permission, SharedRbacService};
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
    let detail = Permission::parse(DETAIL_PERMISSION).map_err(erp_inventory::Error::from)?;
    let adjustment_list =
        Permission::parse(ADJUSTMENT_LIST_PERMISSION).map_err(erp_inventory::Error::from)?;
    let create = Permission::parse(CREATE_PERMISSION).map_err(erp_inventory::Error::from)?;
    let update = Permission::parse(UPDATE_PERMISSION).map_err(erp_inventory::Error::from)?;
    let balance_list = Permission::parse(BALANCE_LIST_PERMISSION).map_err(erp_inventory::Error::from)?;
    let balance_detail = Permission::parse(BALANCE_DETAIL_PERMISSION).map_err(erp_inventory::Error::from)?;
    let movement_list = Permission::parse(MOVEMENT_LIST_PERMISSION).map_err(erp_inventory::Error::from)?;
    let reservation_list =
        Permission::parse(RESERVATION_LIST_PERMISSION).map_err(erp_inventory::Error::from)?;
    let permissions = [
        detail.clone(),
        adjustment_list.clone(),
        create.clone(),
        update.clone(),
        balance_list.clone(),
        balance_detail.clone(),
        movement_list.clone(),
        reservation_list.clone(),
    ];
    let snapshot = rbac
        .role_permission_snapshot(actor.kind(), actor.id(), &permissions)
        .await
        .map_err(|error| map_svc(crate::Error::from(error)))?;
    let enabled = enabled_role_ids(db, snapshot.role_ids(), executor).await?;
    let balance_list_roles = enabled_grants(snapshot.granting_role_ids(&balance_list), &enabled);
    let balance_detail_roles = enabled_grants(snapshot.granting_role_ids(&balance_detail), &enabled);
    let movement_list_roles = enabled_grants(snapshot.granting_role_ids(&movement_list), &enabled);
    let reservation_list_roles = enabled_grants(snapshot.granting_role_ids(&reservation_list), &enabled);
    let adjustment_list_roles = enabled_grants(
        snapshot.granting_role_ids_for_all(&[adjustment_list, detail.clone()]),
        &enabled,
    );
    let read_roles = enabled_grants(snapshot.granting_role_ids(&detail), &enabled);
    let create_roles = enabled_grants(
        snapshot.granting_role_ids_for_all(&[detail.clone(), create]),
        &enabled,
    );
    let update_roles = enabled_grants(snapshot.granting_role_ids_for_all(&[detail, update]), &enabled);
    let user_scopes = db
        .data_scopes()
        .list_by_subject(DataScopeSubjectType::User, actor.id(), executor)
        .await
        .map_err(erp_inventory::Error::from)?;
    let role_scopes = load_role_scopes(
        db,
        [
            balance_list_roles.as_slice(),
            balance_detail_roles.as_slice(),
            movement_list_roles.as_slice(),
            reservation_list_roles.as_slice(),
            adjustment_list_roles.as_slice(),
            read_roles.as_slice(),
            create_roles.as_slice(),
            update_roles.as_slice(),
        ],
        executor,
    )
    .await?;
    let authorization = InventoryAuthorization::from_scopes(
        true,
        scope_from_role_facts(&user_scopes, &balance_list_roles, &role_scopes),
        scope_from_role_facts(&user_scopes, &balance_detail_roles, &role_scopes),
        scope_from_role_facts(&user_scopes, &movement_list_roles, &role_scopes),
        scope_from_role_facts(&user_scopes, &reservation_list_roles, &role_scopes),
        scope_from_role_facts(&user_scopes, &adjustment_list_roles, &role_scopes),
        scope_from_role_facts(&user_scopes, &read_roles, &role_scopes),
        scope_from_role_facts(&user_scopes, &create_roles, &role_scopes),
        scope_from_role_facts(&user_scopes, &update_roles, &role_scopes),
    );
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor)
        .await
        .map_err(|error| map_svc(crate::Error::from(error)))?;
    Ok(authorization)
}

async fn enabled_role_ids(
    db: &Database,
    role_ids: &[String],
    executor: &mut dyn Executor,
) -> erp_inventory::Result<HashSet<String>> {
    Ok(db
        .roles()
        .enabled_roles(role_ids, executor)
        .await
        .map_err(erp_inventory::Error::from)?
        .into_iter()
        .map(|role| role.base.id)
        .collect())
}

fn enabled_grants(role_ids: Vec<String>, enabled: &HashSet<String>) -> Vec<String> {
    role_ids
        .into_iter()
        .filter(|role_id| enabled.contains(role_id))
        .collect()
}

async fn load_role_scopes<const N: usize>(
    db: &Database,
    role_sets: [&[String]; N],
    executor: &mut dyn Executor,
) -> erp_inventory::Result<HashMap<String, Vec<DataScope>>> {
    let mut role_ids = role_sets.into_iter().flatten().cloned().collect::<Vec<_>>();
    role_ids.sort();
    role_ids.dedup();
    let scopes = db
        .data_scopes()
        .list_by_subjects(DataScopeSubjectType::Role, &role_ids, executor)
        .await
        .map_err(erp_inventory::Error::from)?;
    Ok(scopes_by_subject(scopes))
}

fn scope_from_role_facts(
    user_scopes: &[DataScope],
    permitted_role_ids: &[String],
    scopes_by_role: &HashMap<String, Vec<DataScope>>,
) -> WarehouseScope {
    let user = OrganizationCoverage::from_scopes(user_scopes).unwrap_or(OrganizationCoverage::All);
    let mut warehouses = Vec::new();
    for role_id in permitted_role_ids {
        let Some(role) = scopes_by_role
            .get(role_id)
            .and_then(|scopes| OrganizationCoverage::from_scopes(scopes))
        else {
            continue;
        };
        match role.intersect(&user) {
            Some(OrganizationCoverage::All) => return WarehouseScope::company(),
            Some(OrganizationCoverage::Targets(targets)) => warehouses.extend(targets),
            None => {}
        }
    }
    WarehouseScope::from_targets(warehouses)
}

fn scopes_by_subject(scopes: Vec<DataScope>) -> HashMap<String, Vec<DataScope>> {
    let mut grouped = HashMap::new();
    for scope in scopes {
        grouped
            .entry(scope.subject_id.clone())
            .or_insert_with(Vec::new)
            .push(scope);
    }
    grouped
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
    use super::{scope_from_role_facts, scopes_by_subject};
    use erp_core::ids::DataScopeId;
    use erp_identity::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
    use erp_inventory::WarehouseScope;

    fn scope(
        id: &str,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        scope_type: DataScopeType,
        targets: &[&str],
    ) -> DataScope {
        DataScope::new(
            DataScopeId::new(id),
            DataScopeData {
                subject_type,
                subject_id: subject_id.to_string(),
                scope_type,
                scope_targets: targets.iter().map(|item| (*item).to_string()).collect(),
            },
        )
        .unwrap()
    }

    #[test]
    fn role_and_user_scopes_intersect_before_roles_are_unioned() {
        let user = vec![scope(
            "user-scope",
            DataScopeSubjectType::User,
            "user-1",
            DataScopeType::Organization,
            &["warehouse-2"],
        )];
        let role_scopes = scopes_by_subject(vec![
            scope(
                "role-a-scope",
                DataScopeSubjectType::Role,
                "role-a",
                DataScopeType::Organization,
                &["warehouse-1", "warehouse-2"],
            ),
            scope(
                "role-b-scope",
                DataScopeSubjectType::Role,
                "role-b",
                DataScopeType::Company,
                &[],
            ),
        ]);
        let result =
            scope_from_role_facts(&user, &["role-a".to_string(), "role-b".to_string()], &role_scopes);
        assert_eq!(
            result,
            WarehouseScope::from_targets(vec!["warehouse-2".to_string()])
        );
    }

    #[test]
    fn missing_role_scope_fails_closed_even_when_user_scope_is_unrestricted() {
        let result = scope_from_role_facts(&[], &["role-a".to_string()], &std::collections::HashMap::new());
        assert_eq!(result, WarehouseScope::empty());
    }

    #[test]
    fn company_role_scope_is_company_when_user_has_no_explicit_cap() {
        let role_scopes = scopes_by_subject(vec![scope(
            "role-company",
            DataScopeSubjectType::Role,
            "role-a",
            DataScopeType::Company,
            &[],
        )]);
        assert_eq!(
            scope_from_role_facts(&[], &["role-a".to_string()], &role_scopes),
            WarehouseScope::company()
        );
    }
}
