//! 用真实统一授权入口验证库存资格与真实仓库目标；不连接数据库。
#![cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{internal, inventory};
use mongodb::bson::{Document, doc};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{FactValue, ObjectFacts};
use crate::repository::authorization::fixtures::execute;
use crate::service::authorization::{AuthorizationService, rules};
use crate::{AccessTarget, Error, Permission};

fn entries() -> [(Permission, Permission); 8] {
    [
        (internal::INVENTORY_BALANCE_WAREHOUSES, inventory::STOCK_BALANCE_LIST),
        (internal::INVENTORY_BALANCE_DETAIL_WAREHOUSES, inventory::STOCK_BALANCE_DETAIL),
        (internal::INVENTORY_MOVEMENT_WAREHOUSES, inventory::STOCK_MOVEMENT_LIST),
        (internal::INVENTORY_RESERVATION_WAREHOUSES, inventory::STOCK_RESERVATION_LIST),
        (internal::INVENTORY_ADJUSTMENT_WAREHOUSES, inventory::STOCK_ADJUSTMENT_LIST),
        (internal::INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES, inventory::STOCK_ADJUSTMENT_DETAIL),
        (internal::INVENTORY_CREATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_CREATE),
        (internal::INVENTORY_UPDATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_UPDATE),
    ]
}

fn warehouse(id: &str) -> ObjectFacts {
    ObjectFacts {
        warehouse_id: FactValue::Known(Some(id.into())),
        ..object("inventory_warehouse", id, "other")
    }
}

fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("inventory_warehouse".into(), facts);
    builder.build().unwrap()
}

fn scope(identity: &Identity, source: &Permission, ids: &[&str]) {
    identity.role_scopes.lock().unwrap().clear();
    identity.role_scopes.lock().unwrap().insert(
        ("role".into(), source.clone()),
        ScopeClause { warehouse_ids: ids.iter().map(|id| (*id).into()).collect(), ..Default::default() },
    );
}

#[tokio::test]
async fn warehouse_entries_reuse_original_inventory_permissions_and_scope_without_granting_internal_codes() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![warehouse("allowed"), warehouse("other")]));
    let authorization = service(identity.clone(), facts.clone());
    for (entry, source) in entries() {
        scope(&identity, &source, &["allowed"]);
        let required = rules::same_role_permissions(&source);
        identity.state.lock().unwrap().grants = vec![("role".into(), required)];
        let mut executor = SnapshotExecutor::default();
        authorization
            .require(&actor(), &entry, AccessTarget::record("allowed").unwrap(), &mut executor)
            .await
            .unwrap();
        assert!(matches!(
            authorization
                .require(&actor(), &entry, AccessTarget::record("other").unwrap(), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));

        let before = facts.calls.load(Ordering::SeqCst);
        identity.state.lock().unwrap().grants = vec![("role".into(), vec![entry.clone()])];
        assert!(matches!(
            authorization
                .require(&actor(), &entry, AccessTarget::record("allowed").unwrap(), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(facts.calls.load(Ordering::SeqCst), before);
    }
}

#[tokio::test]
async fn adjustment_warehouse_checks_preserve_same_role_detail_and_reject_missing_real_warehouses() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![warehouse("allowed")]));
    let authorization = service(identity.clone(), facts);
    for (entry, source) in entries().into_iter().filter(|(_, source)| {
        [
            inventory::STOCK_ADJUSTMENT_LIST,
            inventory::STOCK_ADJUSTMENT_CREATE,
            inventory::STOCK_ADJUSTMENT_UPDATE,
        ]
        .contains(source)
    }) {
        scope(&identity, &source, &["allowed", "missing"]);
        let mut executor = SnapshotExecutor::default();
        identity.state.lock().unwrap().grants = vec![
            ("role".into(), vec![source.clone()]),
            ("reader".into(), vec![inventory::STOCK_ADJUSTMENT_DETAIL]),
        ];
        assert!(matches!(
            authorization
                .require(&actor(), &entry, AccessTarget::record("allowed").unwrap(), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
        assert!(matches!(
            authorization.query(&actor(), &entry, &mut executor).await,
            Err(Error::Forbidden(_))
        ));

        identity.state.lock().unwrap().grants =
            vec![("role".into(), vec![source, inventory::STOCK_ADJUSTMENT_DETAIL])];
        authorization
            .require(&actor(), &entry, AccessTarget::record("allowed").unwrap(), &mut executor)
            .await
            .unwrap();
        assert!(matches!(
            authorization
                .require(&actor(), &entry, AccessTarget::record("missing").unwrap(), &mut executor)
                .await,
            Err(Error::NotFound(_))
        ));
    }
}

#[tokio::test]
async fn warehouse_targets_cannot_be_fabricated_candidates_or_replace_existing_adjustment_objects() {
    let identity = Arc::new(Identity::default());
    let entry = internal::INVENTORY_CREATE_WAREHOUSES;
    scope(&identity, &inventory::STOCK_ADJUSTMENT_CREATE, &["warehouse"]);
    let authorization = service(identity, Arc::new(Facts::new(vec![warehouse("warehouse")])));
    let mut executor = SnapshotExecutor::default();
    for target in [AccessTarget::Candidate(warehouse("warehouse")), AccessTarget::Operation] {
        assert!(matches!(
            authorization.require(&actor(), &entry, target, &mut executor).await,
            Err(Error::ValidationError(_))
        ));
    }
    assert!(matches!(
        authorization
            .require(
                &actor(),
                &inventory::STOCK_ADJUSTMENT_DETAIL,
                AccessTarget::record("warehouse").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        authorization
            .require(
                &actor(),
                &entry,
                AccessTarget::record("warehouse").unwrap(),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

fn warehouse_rows() -> Vec<Document> {
    vec![
        doc! {"id":"allowed", "warehouse_id":"allowed", "deleted_at":0},
        doc! {"id":"other", "warehouse_id":"other", "deleted_at":0},
        doc! {"id":"deleted", "warehouse_id":"allowed", "deleted_at":1},
    ]
}

#[tokio::test]
async fn warehouse_existence_is_independent_of_empty_inventory_and_empty_scope_never_becomes_unrestricted() {
    let identity = Arc::new(Identity::default());
    let entry = internal::INVENTORY_BALANCE_WAREHOUSES;
    let source = inventory::STOCK_BALANCE_LIST;
    identity.state.lock().unwrap().grants = vec![("role".into(), vec![source.clone()])];
    let authorization = service(identity.clone(), Arc::new(Facts::new(vec![warehouse("allowed")])));
    for (ids, expected) in [(vec!["allowed"], vec!["allowed"]), (vec![], vec![])] {
        scope(&identity, &source, &ids);
        let mut executor = SnapshotExecutor::default();
        let access = authorization.query(&actor(), &entry, &mut executor).await.unwrap();
        let warehouses = execute(warehouse_rows(), access.stages(), &BTreeMap::new(), &doc! {});
        assert_eq!(warehouses.iter().map(|row| row.get_str("id").unwrap()).collect::<Vec<_>>(), expected);
        authorization.validate(&access, &mut executor).await.unwrap();
        let inventory = authorization.query(&actor(), &source, &mut executor).await.unwrap();
        let rows = execute(Vec::new(), inventory.stages(), &BTreeMap::new(), &doc! {});
        assert!(rows.is_empty());
        authorization.validate(&inventory, &mut executor).await.unwrap();
        let record = authorization
            .require(&actor(), &entry, AccessTarget::record("allowed").unwrap(), &mut executor)
            .await;
        if ids.is_empty() {
            assert!(matches!(record, Err(Error::Forbidden(_))));
        } else {
            record.unwrap();
        }
    }
}

#[tokio::test]
async fn warehouse_delivery_then_inventory_query_uses_a_new_nontransaction_read_stage() {
    let identity = Arc::new(Identity::default());
    let entry = internal::INVENTORY_BALANCE_WAREHOUSES;
    scope(&identity, &inventory::STOCK_BALANCE_LIST, &["allowed"]);
    let authorization = service(identity, Arc::new(Facts::new(vec![warehouse("allowed")])));
    let mut executor = ReadExecutor::default();
    let warehouse = authorization.query(&actor(), &entry, &mut executor).await.unwrap();
    authorization.validate(&warehouse, &mut executor).await.unwrap();

    let inventory =
        authorization.query(&actor(), &inventory::STOCK_BALANCE_LIST, &mut executor).await.unwrap();
    assert!(matches!(warehouse.ensure_current(&mut executor), Err(Error::ValidationError(_))));
    authorization.validate(&inventory, &mut executor).await.unwrap();

    executor.next_read();
    assert!(matches!(inventory.ensure_current(&mut executor), Err(Error::ValidationError(_))));
}
