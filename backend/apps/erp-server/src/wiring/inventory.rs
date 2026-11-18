//! 库存授权、审计与外部事实 adapter。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_catalog::CatalogExt;
use erp_catalog::repository::prelude::*;
use erp_core::ids::SkuId;
use erp_fulfillment::repository::FulfillmentExt;
use erp_fulfillment_execution::inventory_adjustment::InventoryAdjustmentProcess;
use erp_fulfillment_execution::workflow::FulfillmentWorkflowServices;
use erp_identity::{AccessTarget, AuthorizationService, Error as IdentityError, SharedRbacService};
use erp_inventory::{
    AuthorizationPort, CatalogFactsPort, Error as InventoryError, FulfillmentFactsPort, InventoryAction,
    InventoryAuditPort, InventoryAuthorization, InventoryQuery, InventoryService, PreparedInventoryAudit,
    ReceiptNoFact, Result as InventoryResult, SkuFact, SkuRevisionFact, WarehouseFact, WarehouseFactsPort,
    WarehouseRevisionFact,
};
use erp_permissions::{internal, inventory};
use erp_warehouse::WarehouseExt;
use erp_warehouse::authorization::WarehouseAuthorization;
use mongodb::Database;
use persistence_core::Executor;

use super::approval_runtime;
use super::audit_conversion::PreparedAudit;
use super::errors::Error;
use crate::wiring::workflow::work_item_service;

mod people;

pub use people::MongoInventoryPeopleFacts;

/// 使用共享统一授权服务检查库存对象及真实仓库。
#[derive(Clone)]
pub struct MongoInventoryAuthorization {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl MongoInventoryAuthorization {
    /// 绑定业务库与完整统一授权服务。
    /// 不执行 I/O 的库存授权 adapter。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 包装为库存域可注入的共享授权 Port。
    /// 保留同一授权实例的共享 Port。
    pub fn shared(db: Database, authorization: Arc<AuthorizationService>) -> Arc<dyn AuthorizationPort> {
        Arc::new(Self::new(db, authorization))
    }
}

#[async_trait]
impl AuthorizationPort for MongoInventoryAuthorization {
    async fn require_adjustment(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> InventoryResult<()> {
        self.authorization
            .require(actor, &inventory::STOCK_ADJUSTMENT_DETAIL, AccessTarget::record(id)?, executor)
            .await
            .map_err(Into::into)
    }

    async fn require_adjustment_update(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> InventoryResult<()> {
        self.authorization
            .require(actor, &inventory::STOCK_ADJUSTMENT_UPDATE, AccessTarget::record(id)?, executor)
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        actor: &AuditActor,
        action: InventoryAction,
        executor: &mut dyn Executor,
    ) -> InventoryResult<InventoryAuthorization> {
        authorize_inventory(&self.db, &self.authorization, actor, action, executor).await
    }

    async fn require_warehouse(
        &self,
        actor: &AuditActor,
        action: InventoryAction,
        warehouse: &str,
        executor: &mut dyn Executor,
    ) -> InventoryResult<()> {
        warehouse_authorization(&self.db, &self.authorization, action)
            .require(actor, warehouse, executor)
            .await
            .map_err(Into::into)
    }

    async fn allowed_warehouses(
        &self,
        actor: &AuditActor,
        action: InventoryAction,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> InventoryResult<BTreeSet<String>> {
        match warehouse_authorization(&self.db, &self.authorization, action)
            .ids_in(actor, ids, executor)
            .await
        {
            Ok(ids) => Ok(ids.into_iter().collect()),
            Err(IdentityError::Forbidden(_)) => Ok(BTreeSet::new()),
            Err(error) => Err(error.into()),
        }
    }
}

/// 在调用方执行器上只生成当前读取动作的完整库存凭证。
/// db、authorization 为共享基础设施，actor、action 为真实身份和当前动作，executor 为原执行器。
/// 当前动作的查询凭证；有效账号缺少功能资格时不签发查询。
/// 非读取动作、身份设施、范围或查询依赖错误时拒绝。
pub async fn authorize_inventory(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    action: InventoryAction,
    executor: &mut dyn Executor,
) -> InventoryResult<InventoryAuthorization> {
    if matches!(action, InventoryAction::Create | InventoryAction::Update) {
        return Err(InventoryError::ValidationError("库存授权查询只接受读取动作".into()));
    }
    if authorization.permissions(actor, &[action.permission()], executor).await?.is_none() {
        return Ok(InventoryAuthorization::inactive());
    }
    let query = inventory_query(db, authorization, actor, action, executor).await?;
    Ok(InventoryAuthorization::new(true, query.map(|query| (action, query))))
}

// 为库存接线提供固定的仓库授权入口；范围复用和资格判断仍交给身份服务，不在接线时授权。
fn warehouse_authorization(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    action: InventoryAction,
) -> WarehouseAuthorization {
    let permission = match action {
        InventoryAction::BalanceList => internal::INVENTORY_BALANCE_WAREHOUSES,
        InventoryAction::BalanceDetail => internal::INVENTORY_BALANCE_DETAIL_WAREHOUSES,
        InventoryAction::MovementList => internal::INVENTORY_MOVEMENT_WAREHOUSES,
        InventoryAction::ReservationList => internal::INVENTORY_RESERVATION_WAREHOUSES,
        InventoryAction::AdjustmentList => internal::INVENTORY_ADJUSTMENT_WAREHOUSES,
        InventoryAction::AdjustmentDetail => internal::INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES,
        InventoryAction::Create => internal::INVENTORY_CREATE_WAREHOUSES,
        InventoryAction::Update => internal::INVENTORY_UPDATE_WAREHOUSES,
    };
    WarehouseAuthorization::new(db.clone(), Arc::clone(authorization), permission)
}

// 读取库存前先取得真实仓库存在性事实，然后进入唯一的库存查询阶段。
async fn inventory_query(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    actor: &AuditActor,
    action: InventoryAction,
    executor: &mut dyn Executor,
) -> InventoryResult<Option<InventoryQuery>> {
    let has_warehouse =
        match warehouse_authorization(db, authorization, action).has_any(actor, executor).await {
            Ok(has_warehouse) => has_warehouse,
            Err(IdentityError::Forbidden(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
    let compiled = match authorization.query(actor, &action.permission(), executor).await {
        Ok(compiled) => compiled,
        Err(IdentityError::Forbidden(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    Ok(Some(InventoryQuery::new(Arc::clone(authorization), compiled, !has_warehouse)))
}

/// 把库存审计事实写入 `erp-audit` 的 Mongo adapter。
#[derive(Clone)]
pub struct MongoInventoryAudit {
    db: Database,
}

impl MongoInventoryAudit {
    /// 为 `MongoInventoryAudit` 绑定审计日志库，供后续业务审计写入；构造时不写库。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的共享审计 Port。
    /// 返回共享的库存审计 Port。
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
        Ok(PreparedInventoryAudit::from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedInventoryAudit,
        executor: &mut dyn Executor,
    ) -> erp_inventory::Result<()> {
        let log = audit.to_log().map_err(map_audit_to_inventory)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_inventory)?;
        Ok(())
    }
}

/// 供库存列表与详情补齐仓库身份的 adapter。
#[derive(Clone)]
pub struct MongoInventoryWarehouseFacts {
    db: Database,
}

impl MongoInventoryWarehouseFacts {
    /// 绑定仓库集合所在数据库，构造时不读取。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的仓库事实 Port。
    /// 返回共享的仓库事实 Port。
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
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的商品事实 Port。
    /// 返回共享的商品事实 Port。
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
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为库存域可注入的履约事实 Port。
    /// 返回共享的履约事实 Port。
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
/// 返回库存查询服务。
pub fn inventory_service(db: Database, authorization: Arc<AuthorizationService>) -> InventoryService {
    InventoryService::new(
        db.clone(),
        MongoInventoryAuthorization::shared(db.clone(), authorization),
        MongoInventoryWarehouseFacts::shared(db.clone()),
        MongoInventoryCatalogFacts::shared(db.clone()),
        MongoInventoryFulfillmentFacts::shared(db.clone()),
        MongoInventoryAudit::shared(db.clone()),
        MongoInventoryPeopleFacts::shared(db),
    )
}

/// 装配拥有跨域事务的库存调整流程。
/// 返回库存调整流程。
pub fn inventory_adjustment_process(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> InventoryAdjustmentProcess {
    let workflow = Arc::new(FulfillmentWorkflowServices::new(
        work_item_service(db.clone(), rbac.clone(), authorization.clone()),
        approval_runtime(db.clone(), rbac.clone(), authorization.clone()),
    ));
    let inventory = Arc::new(inventory_service(db.clone(), authorization.clone()));
    InventoryAdjustmentProcess::new(db, authorization, workflow, inventory)
}

fn map_audit_to_inventory(error: erp_audit::Error) -> erp_inventory::Error {
    map_svc(Error::from(error))
}

fn map_svc(error: Error) -> erp_inventory::Error {
    match error {
        Error::Internal(message) => erp_inventory::Error::Internal(message),
        Error::NotFound(message) => erp_inventory::Error::NotFound(message),
        Error::ValidationError(message) => erp_inventory::Error::ValidationError(message),
        Error::InvalidInput(message) => erp_inventory::Error::InvalidInput(message),
        Error::BusinessLogicError(message) => erp_inventory::Error::BusinessLogicError(message),
        Error::ConflictError(message) => erp_inventory::Error::ConflictError(message),
        Error::ReceiptDuplicate(error) => erp_inventory::Error::ReceiptDuplicate(error),
        Error::TransientTransaction(error) => erp_inventory::Error::TransientTransaction(error),
        Error::Forbidden(message) => erp_inventory::Error::Forbidden(message),
        Error::Unauthenticated(message) => erp_inventory::Error::Unauthenticated(message),
        Error::Logic(error) => erp_inventory::Error::Logic(error),
        Error::OutcomeUnknown(error) => erp_inventory::Error::OutcomeUnknown(error),
        Error::RepositoryError(error) => erp_inventory::Error::RepositoryError(error),
        other => erp_inventory::Error::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use entity_core::BaseModel;
    use erp_core::AccountKind;

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
            let prepared = PreparedInventoryAudit::from_log(&log);
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
            let prepared = PreparedInventoryAudit::from_log(&log);
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
            .resource_log("stock_adjustment.update", "stock_adjustment", "adjustment-1".into())
            .unwrap();
        let mut prepared = PreparedInventoryAudit::from_log(&log);
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
            .resource_log("stock_adjustment.update", "stock_adjustment", "adjustment-1".into())
            .unwrap();
        let mut prepared = PreparedInventoryAudit::from_log(&log);
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
