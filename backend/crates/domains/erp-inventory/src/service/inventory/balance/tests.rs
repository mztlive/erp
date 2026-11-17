#![cfg(test)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use erp_core::AccountKind;
use erp_core::ids::{SkuId, StockMovementId, WarehouseId};
use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::testing::fixed_authorization;
use erp_identity::{AccessMapping, Result as IdentityResult};
use mongodb::Client;
use mongodb::bson::{Bson, doc};
use persistence_core::testing::SnapshotExecutor;

use super::*;
use crate::ports::{
    AuthorizationPort, FailClosedAdjustmentPeopleFactsPort, FailClosedAuditPort, FailClosedCatalogFacts,
    FailClosedFulfillmentFacts, FailClosedWarehouseFacts, InventoryQuery,
};
use crate::repository::inventory::authorization::InventoryMapping;

struct MissingReadQuery(bool);

#[async_trait]
impl AuthorizationPort for MissingReadQuery {
    async fn require_adjustment_update(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<()> {
        panic!("余额详情不得检查调整单修改资格")
    }

    async fn require_adjustment(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<()> {
        panic!("无详情凭证不得检查调整单")
    }

    async fn authorize(
        &self,
        _: &AuditActor,
        action: InventoryAction,
        executor: &mut dyn Executor,
    ) -> Result<InventoryAuthorization> {
        assert_eq!(action, InventoryAction::BalanceDetail);
        assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
        Ok(InventoryAuthorization::new(self.0, None))
    }

    async fn require_warehouse(
        &self,
        _: &AuditActor,
        _: InventoryAction,
        _: &str,
        _: &mut dyn Executor,
    ) -> Result<()> {
        panic!("无详情凭证不得读取附加动作资格")
    }

    async fn allowed_warehouses(
        &self,
        _: &AuditActor,
        _: InventoryAction,
        _: &[String],
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        panic!("无详情凭证不得枚举仓库动作资格")
    }
}

#[tokio::test]
async fn balance_detail_rejects_inactive_and_unqualified_actors_before_business_reads() {
    let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("balance_detail_authorization");
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    for active in [false, true] {
        let service = InventoryService::new(
            db.clone(),
            Arc::new(MissingReadQuery(active)),
            Arc::new(FailClosedWarehouseFacts),
            Arc::new(FailClosedCatalogFacts),
            Arc::new(FailClosedFulfillmentFacts),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedAdjustmentPeopleFactsPort),
        );
        let result =
            service.read_balance_detail("unread-balance", &actor, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::NotFound(message)) if message == "库存余额不存在"));
    }
}

struct WarehouseModel(QueryModel);

impl AccessMapping for WarehouseModel {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":0}))
    }
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<QueryFragment> {
        unreachable!("公司范围不执行仓库维度谓词")
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<RelationMapping> {
        unreachable!("仓库没有外部来源")
    }
}

async fn balance_authorization(executor: &mut dyn Executor) -> InventoryAuthorization {
    let permission = InventoryAction::BalanceList.permission();
    let warehouse = Arc::new(WarehouseModel(QueryModel {
        key: QueryModelKey::parse("warehouse").unwrap(),
        resource: "warehouse".into(),
        collection: "warehouses".into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts: [FactKind::Warehouse].into(),
    }));
    let mapping = InventoryMapping::shared(InventoryAction::BalanceList, warehouse).unwrap();
    let service = fixed_authorization(
        &[permission.clone()],
        ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        },
        Vec::new(),
        vec![(permission.clone(), mapping)],
    )
    .unwrap();
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    let compiled = service.query(&actor, &permission, executor).await.unwrap();
    InventoryAuthorization::new(
        true,
        Some((InventoryAction::BalanceList, InventoryQuery::new(service, compiled, false))),
    )
}

struct ButtonPhase {
    main: InventoryQuery,
    calls: AtomicUsize,
}
#[async_trait]
impl AuthorizationPort for ButtonPhase {
    async fn require_adjustment_update(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<()> {
        unreachable!()
    }
    async fn require_adjustment(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<()> {
        unreachable!()
    }
    async fn authorize(
        &self,
        _: &AuditActor,
        _: InventoryAction,
        _: &mut dyn Executor,
    ) -> Result<InventoryAuthorization> {
        unreachable!()
    }
    async fn require_warehouse(
        &self,
        _: &AuditActor,
        _: InventoryAction,
        _: &str,
        _: &mut dyn Executor,
    ) -> Result<()> {
        unreachable!()
    }
    async fn allowed_warehouses(
        &self,
        _: &AuditActor,
        action: InventoryAction,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        assert_eq!(action, InventoryAction::Create);
        assert!(matches!(self.main.ensure_current(executor), Err(Error::ValidationError(_))));
        self.calls.fetch_add(1, Ordering::SeqCst);
        executor.next_read();
        Ok(ids.iter().filter(|id| id.as_str() == "allowed").cloned().collect())
    }
}

#[tokio::test]
async fn balance_page_delivers_main_read_before_starting_independent_buttons() {
    let mut executor = SnapshotExecutor::default();
    let main = balance_authorization(&mut executor).await;
    let port = ButtonPhase {
        main: main.query(InventoryAction::BalanceList).unwrap().clone(),
        calls: AtomicUsize::new(0),
    };
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    let ids = vec!["allowed".into(), "other".into()];
    let buttons = finish_balance_page(&main, &port, &actor, &ids, &mut executor).await.unwrap();
    assert_eq!(buttons, BTreeSet::from(["allowed".into()]));
    assert_eq!(port.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(main.validate(&mut executor).await, Err(Error::ValidationError(_))));
}

#[tokio::test]
async fn failed_main_delivery_stops_before_independent_buttons() {
    let mut executor = SnapshotExecutor::default();
    let main = balance_authorization(&mut executor).await;
    let port = ButtonPhase {
        main: main.query(InventoryAction::BalanceList).unwrap().clone(),
        calls: AtomicUsize::new(0),
    };
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    executor.next_read();
    let result = finish_balance_page(&main, &port, &actor, &["allowed".into()], &mut executor).await;
    assert!(matches!(result, Err(Error::ValidationError(_))));
    assert_eq!(port.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn balance_create_action_is_only_emitted_from_server_authorization() {
    assert!(balance_allowed_actions(false).is_empty());
    assert_eq!(balance_allowed_actions(true), vec!["CREATE_ADJUSTMENT".to_string()]);
}

#[test]
fn balance_view_keeps_stock_facts_and_requires_existing_warehouse_for_action() {
    let mut enrichments = BalanceEnrichments {
        warehouses: HashMap::new(),
        warehouse_revisions: HashMap::new(),
        skus: HashMap::new(),
        sku_revisions: HashMap::new(),
        movements: HashMap::new(),
    };
    for has_warehouse in [false, true] {
        if has_warehouse {
            enrichments.warehouses.insert(
                "warehouse".into(),
                WarehouseFact {
                    id: "warehouse".into(),
                    warehouse_code: "WH-1".into(),
                    current_revision_id: Some("revision".into()),
                },
            );
            enrichments.warehouse_revisions.insert(
                "revision".into(),
                WarehouseRevisionFact { id: "revision".into(), name: "一号仓".into() },
            );
        }
        let view = enrich_balance_view(
            StockBalanceRow {
                id: "balance".into(),
                warehouse_id: WarehouseId::new("warehouse"),
                sku_id: SkuId::new("sku"),
                on_hand_quantity: "5".parse().unwrap(),
                reserved_quantity: "2".parse().unwrap(),
                available_quantity: "3".parse().unwrap(),
                version: 7,
                last_movement_id: Some(StockMovementId::new("unresolved-movement")),
            }
            .into(),
            &enrichments,
            true,
            true,
        );
        assert_eq!(view.id, "balance");
        assert_eq!(view.warehouse_id, "warehouse");
        assert_eq!(view.sku_id, "sku");
        assert_eq!(view.on_hand_quantity.to_string(), "5");
        assert_eq!(view.reserved_quantity.to_string(), "2");
        assert_eq!(view.available_quantity.to_string(), "3");
        assert_eq!(view.version, "7");
        assert_eq!(view.last_movement_id.as_deref(), Some("unresolved-movement"));
        assert!(view.last_movement_at.is_none());
        assert!(view.last_movement_type.is_none());
        assert!(view.has_active_reservation);
        assert!(view.sku_code.is_empty());
        assert!(view.sku_name.is_empty());
        assert_eq!(view.warehouse_code, if has_warehouse { "WH-1" } else { "" });
        assert_eq!(view.warehouse_name, if has_warehouse { "一号仓" } else { "" });
        assert_eq!(
            view.allowed_actions,
            if has_warehouse { vec!["CREATE_ADJUSTMENT".to_string()] } else { vec![] }
        );
    }
}
