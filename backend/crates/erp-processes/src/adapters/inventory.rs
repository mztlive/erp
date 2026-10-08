//! 库存授权、审计与外部事实 adapter。

use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroU32;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_audit::{AuditActorLogs, AuditLog, AuditLogData, prepare_business_log};
use erp_catalog::CatalogExt;
use erp_catalog::repository::prelude::*;
use erp_core::ids::SkuId;
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeBatch, DataScopeService};
use erp_identity::{Error as IdentityError, Permission, SharedRbacService};
use erp_inventory::{
    AuthorizationPort, CatalogFactsPort, FulfillmentFactsPort, InventoryAuditPort, InventoryAuthorization,
    InventoryScopeMeta, InventoryService, PreparedInventoryAudit, ReceiptNoFact, SkuFact, SkuRevisionFact,
    WarehouseFact, WarehouseFactsPort, WarehouseRevisionFact, WarehouseScope,
};
use erp_warehouse::WarehouseExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::adapters::workflow::workflow_auth;
use crate::audit::persist_log;

mod people;

pub use people::MongoInventoryPeopleFacts;

const DETAIL_PERMISSION: &str = "stock_adjustment:detail";
const ADJUSTMENT_LIST_PERMISSION: &str = "stock_adjustment:list";
const CREATE_PERMISSION: &str = "stock_adjustment:create";
const UPDATE_PERMISSION: &str = "stock_adjustment:update";
const BALANCE_LIST_PERMISSION: &str = "stock_balance:list";
const BALANCE_DETAIL_PERMISSION: &str = "stock_balance:detail";
const MOVEMENT_LIST_PERMISSION: &str = "stock_movement:list";
const RESERVATION_LIST_PERMISSION: &str = "stock_reservation:list";
/// 按原授权次序保留八个库存资源动作槽与同角色详情资格要求。
const INVENTORY_OPERATIONS: [(&str, bool); 8] = [
    (BALANCE_LIST_PERMISSION, false),
    (BALANCE_DETAIL_PERMISSION, false),
    (MOVEMENT_LIST_PERMISSION, false),
    (RESERVATION_LIST_PERMISSION, false),
    (ADJUSTMENT_LIST_PERMISSION, true),
    (DETAIL_PERMISSION, false),
    (CREATE_PERMISSION, true),
    (UPDATE_PERMISSION, true),
];

/// 从身份事实计算库存仓库范围的 Mongo adapter。
#[derive(Clone)]
pub struct MongoInventoryAuthorization {
    db: Database,
    rbac: SharedRbacService,
}

impl MongoInventoryAuthorization {
    /// 绑定身份数据库与共享 RBAC，构造时不解析范围。
    ///
    /// # 参数
    /// * `db` - 身份与库存集合所在数据库。
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

    /// 包装为库存域可注入的授权 Port。
    ///
    /// # 参数
    /// * `db` - 身份与库存集合所在数据库。
    /// * `rbac` - 现有 RBAC 快照服务。
    ///
    /// # 返回
    /// 返回共享的库存授权 Port。
    ///
    /// # 错误
    /// 不返回错误。
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

/// 在调用方执行器快照上计算八项库存仓库范围。
///
/// 账号不能登录时返回未激活授权，不继续解析范围。
///
/// # 参数
/// * `db` - 身份与库存集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
/// * `actor` - 已认证操作人。
/// * `executor` - 调用方选择的执行器。
///
/// # 返回
/// 返回八项操作的仓库范围及结存、流水、调整列表元数据。无单项权限时该项为空范围。
///
/// # 错误
/// 登录资格、策略或数据范围查询失败时返回映射后的库存错误。仓库目标超过 20000 时返回校验错误。
///
/// # Panics
/// 固定库存权限码无法解析，或八项范围没有按顺序取完时 panic。
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
    let permissions = INVENTORY_OPERATIONS
        .iter()
        .map(|(code, _)| Permission::parse(*code).expect("固定库存权限合法"))
        .collect::<Vec<_>>();
    let mut batch = service.batch(actor, &permissions, executor);
    let mut scopes = Vec::with_capacity(INVENTORY_OPERATIONS.len());
    let mut metas = Vec::with_capacity(INVENTORY_OPERATIONS.len());
    for (code, requires_detail) in INVENTORY_OPERATIONS {
        let (scope, meta) = inventory_scope(&mut batch, code, requires_detail).await?;
        scopes.push(scope);
        metas.push(meta);
    }
    let mut scopes = scopes.into_iter();
    let mut metas = metas.into_iter();
    let mut next = || scopes.next().expect("八项已解析范围");
    let mut next_meta = || metas.next().expect("八项已解析范围");
    let balance_meta = next_meta();
    let _ = next_meta();
    let movement_meta = next_meta();
    let _ = next_meta();
    let adjustment_meta = next_meta();
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
    )
    .with_list_meta(balance_meta, movement_meta, adjustment_meta))
}

/// 库存查询共用仓库政策，保留各资源动作的授权槽，避免合并存量范围扩大权限。
///
/// 库存调整创建、更新及列表沿用同角色完整详情权限要求；仓库目录范围不参与。
///
/// # Panics
/// 固定权限码不含冒号或无法解析时 panic。
async fn inventory_scope(
    batch: &mut DataScopeBatch<'_>,
    code: &str,
    requires_detail: bool,
) -> erp_inventory::Result<(WarehouseScope, InventoryScopeMeta)> {
    let (resource, action) = code.split_once(':').expect("固定库存权限合法");
    let extra = requires_detail
        .then(|| Permission::parse(DETAIL_PERMISSION).expect("固定权限合法"))
        .into_iter()
        .collect::<Vec<_>>();
    let access = match batch.resolve_permissions(resource, action, &extra).await {
        Ok(access) => access,
        Err(IdentityError::Forbidden(_)) => {
            return Ok((WarehouseScope::empty(), InventoryScopeMeta::empty()));
        },
        Err(error) => return Err(map_svc(error.into())),
    };
    Ok((warehouse_scope(&access)?, scope_meta(&access)))
}

fn scope_meta(access: &AuthorizedDataScope) -> InventoryScopeMeta {
    InventoryScopeMeta::new(
        access.scope_version.clone(),
        access.policy_version,
        access.organizations.version,
        access.as_of.as_utc().to_rfc3339(),
    )
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
    Ok(WarehouseScope::from_targets(ids.iter().filter(|id| allows(Some(id.as_str()))).cloned().collect()))
}

/// 把库存审计事实写入 `erp-audit` 的 Mongo adapter。
#[derive(Clone)]
pub struct MongoInventoryAudit {
    db: Database,
}

impl MongoInventoryAudit {
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

    /// 包装为库存域可注入的共享审计 Port。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回共享的库存审计 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_to_inventory)?;
        Ok(prepared_inventory_audit(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedInventoryAudit,
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<()> {
        let log = audit_log_from_inventory(audit).map_err(map_audit_to_inventory)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_inventory)?;
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
    .with_actor_name_snapshot(
        log.structured_event.as_ref().and_then(|event| event.actor_name_snapshot.clone()),
    )
    .with_request_id(log.structured_event.as_ref().and_then(|event| event.request_id.clone()))
    .with_event_sequence(
        log.structured_event.as_ref().map(|event| event.event_sequence).unwrap_or(NonZeroU32::MIN),
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
    prepare_business_log(&log)?
        .with_actor_name_snapshot(audit.actor_name_snapshot.clone())?
        .with_request_id(audit.request_id.clone())?
        .with_event_sequence(audit.event_sequence.get())
}

/// 供库存列表与详情补齐仓库身份的 adapter。
#[derive(Clone)]
pub struct MongoInventoryWarehouseFacts {
    db: Database,
}

impl MongoInventoryWarehouseFacts {
    /// 绑定仓库集合所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 仓库集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的仓库事实 Port。
    ///
    /// # 参数
    /// * `db` - 仓库集合所在数据库。
    ///
    /// # 返回
    /// 返回共享的仓库事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn WarehouseFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl WarehouseFactsPort for MongoInventoryWarehouseFacts {
    async fn warehouse_exists(&self, id: &str, executor: &mut dyn Executor) -> erp_inventory::Result<bool> {
        Ok(self.db.warehouses().find_by_id(id, executor).await.map_err(erp_inventory::Error::from)?.is_some())
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
                    WarehouseRevisionFact { id: revision.base.id, name: revision.name },
                )
            })
            .collect())
    }
}

/// 供库存列表与详情补齐 SKU 身份的 adapter。
#[derive(Clone)]
pub struct MongoInventoryCatalogFacts {
    db: Database,
}

impl MongoInventoryCatalogFacts {
    /// 绑定商品 SKU 集合所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 商品 SKU 集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的商品事实 Port。
    ///
    /// # 参数
    /// * `db` - 商品 SKU 集合所在数据库。
    ///
    /// # 返回
    /// 返回共享的商品事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
        self.db.catalog().inventory_sku_ids(q, executor).await.map_err(erp_inventory::Error::from)
    }

    async fn skus_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<HashMap<String, SkuFact>> {
        let sku_ids = ids.iter().map(|id| SkuId::new(id.clone())).collect::<Vec<_>>();
        let skus =
            self.db.skus().find_by_ids(&sku_ids, executor).await.map_err(erp_inventory::Error::from)?;
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

/// 供库存流水视图读取采购收货单号的 adapter。
#[derive(Clone)]
pub struct MongoInventoryFulfillmentFacts {
    db: Database,
}

impl MongoInventoryFulfillmentFacts {
    /// 绑定采购收货集合所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 采购收货集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的履约事实 Port。
    ///
    /// # 参数
    /// * `db` - 采购收货集合所在数据库。
    ///
    /// # 返回
    /// 返回共享的履约事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
                    ReceiptNoFact { id: receipt.base.id, receipt_no: receipt.receipt_no },
                )
            })
            .collect())
    }
}

/// 装配带授权、仓库、商品、收货、审计与人员 adapter 的库存查询服务。
///
/// # 参数
/// * `db` - 库存及相关集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
///
/// # 返回
/// 返回库存查询服务。
///
/// # 错误
/// 不返回错误。
pub fn inventory_service(db: Database, rbac: SharedRbacService) -> InventoryService {
    InventoryService::new(
        db.clone(),
        MongoInventoryAuthorization::shared(db.clone(), rbac),
        MongoInventoryWarehouseFacts::shared(db.clone()),
        MongoInventoryCatalogFacts::shared(db.clone()),
        MongoInventoryFulfillmentFacts::shared(db.clone()),
        MongoInventoryAudit::shared(db.clone()),
        MongoInventoryPeopleFacts::shared(db),
    )
}

/// 装配拥有跨域事务的库存调整流程。
///
/// # 参数
/// * `db` - 库存调整使用的数据库。
/// * `rbac` - 现有 RBAC 快照服务。
///
/// # 返回
/// 返回库存调整流程。
///
/// # 错误
/// 不返回错误。
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
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};

    use super::*;

    #[test]
    fn prepared_audit_preserves_original_metadata_and_actor_name_snapshot() {
        for name in [Some("发生时名称".to_string()), None] {
            let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
                .with_actor_name_snapshot(name.clone())
                .unwrap();
            let domain_prepared = PreparedInventoryAudit::resource(
                actor.clone(),
                "stock_adjustment.update",
                "stock_adjustment",
                "adjustment-1".into(),
                None,
            )
            .unwrap();
            assert_eq!(domain_prepared.actor_name_snapshot, name);
            let mut log = actor
                .clone()
                .resource_log_with_id(
                    "audit-original".into(),
                    "stock_adjustment.update",
                    "stock_adjustment",
                    "adjustment-1".into(),
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
            let prepared = prepared_inventory_audit(&log);
            let renamed = actor.with_actor_name_snapshot(Some("当前名称".into())).unwrap();
            assert_eq!(renamed.actor_name_snapshot(), Some("当前名称"));
            let restored = audit_log_from_inventory(&prepared).unwrap();
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
            let domain_prepared = PreparedInventoryAudit::resource(
                actor.clone(),
                "stock_adjustment.update",
                "stock_adjustment",
                "adjustment-1".into(),
                None,
            )
            .unwrap();
            assert_eq!(domain_prepared.request_id, request_id);
            assert_eq!(domain_prepared.event_sequence, NonZeroU32::MIN);
            let log = actor
                .clone()
                .resource_log_with_id(
                    "request-audit-original".into(),
                    "stock_adjustment.update",
                    "stock_adjustment",
                    "adjustment-1".into(),
                    None,
                )
                .unwrap()
                .with_event_sequence(4)
                .unwrap();
            let prepared = prepared_inventory_audit(&log);
            let changed = actor.with_request_id(Some("request-current".into())).unwrap();
            assert_eq!(changed.request_id(), Some("request-current"));
            let restored = audit_log_from_inventory(&prepared).unwrap();
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
            .resource_log("stock_adjustment.update", "stock_adjustment", "adjustment-1".into())
            .unwrap();
        let mut prepared = prepared_inventory_audit(&log);
        prepared.message = Some("body=private-request-body;token=private-token".into());
        let restored = audit_log_from_inventory(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request-body"));
        assert!(!serialized.contains("private-token"));
        for request_id in ["request\nforged".to_string(), "r".repeat(129)] {
            prepared.request_id = Some(request_id);
            assert!(audit_log_from_inventory(&prepared).is_err());
        }
    }

    #[test]
    fn prepared_audit_rejects_unsafe_names_and_drops_unprojected_message() {
        let log = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("发生时名称".into()))
            .unwrap()
            .resource_log("stock_adjustment.update", "stock_adjustment", "adjustment-1".into())
            .unwrap();
        let mut prepared = prepared_inventory_audit(&log);
        prepared.message = Some("token=private-request;bank=private-bank".into());
        let restored = audit_log_from_inventory(&prepared).unwrap();
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("private-bank"));
        assert!(restored.message.as_deref().unwrap().contains("发生时名称"));
        prepared.actor_name_snapshot = Some("非法\n名称".into());
        assert!(audit_log_from_inventory(&prepared).is_err());
        prepared.actor_name_snapshot = Some("名".repeat(129));
        assert!(audit_log_from_inventory(&prepared).is_err());
        prepared.actor_id = " ".into();
        prepared.action = " ".into();
        assert!(audit_log_from_inventory(&prepared).unwrap_err().to_string().contains("操作人ID不能为空"));
    }

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
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
        };
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::empty());
        access
            .scope
            .role_clauses
            .push(ScopeClause { org_unit_ids: BTreeSet::from(["same-id".into()]), ..Default::default() });
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::empty());
        access.scope.role_clauses.push(ScopeClause { company: true, ..Default::default() });
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::company());
        access.scope.user_limit =
            Some(ScopeClause { warehouse_ids: BTreeSet::from(["warehouse-a".into()]), ..Default::default() });
        assert_eq!(
            warehouse_scope(&access).unwrap(),
            WarehouseScope::from_targets(vec!["warehouse-a".into()])
        );
        access.scope.user_limit = Some(ScopeClause::default());
        assert_eq!(warehouse_scope(&access).unwrap(), WarehouseScope::empty());
    }
}
