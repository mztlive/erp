#![cfg(test)]
//! 银行账号揭示、退款冲正提交、整账资格、库存仓库及库存调整授权测试。
use std::collections::BTreeMap;

use erp_permissions::{
    Permission, cost, internal, inventory, party, payable, policy_permission, receivable, returns,
    sales_order,
};
use mongodb::bson::{Document, doc};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, ReadExecutor};

use super::fixtures::{AuthHarness, attach_relation, candidate, object};
use crate::Error;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::repository::authorization::fixtures::execute;
use crate::service::authorization::rules;

// ── 银行账号字段揭示 ──────────────────────────────────────────────────────────

#[tokio::test]
async fn bank_reveal_allows_current_field_qualifications_only_in_snapshot() {
    let h = AuthHarness::with_facts("sales_inquiry", vec![]);
    h.grants(&[("bank-reader", &[party::BANK_ACCOUNT_DETAIL, payable::PAYMENT_RECIPIENT_REVEAL])]);
    let mut ex = SnapshotExecutor::default();
    for permission in [party::BANK_ACCOUNT_DETAIL, payable::PAYMENT_RECIPIENT_REVEAL] {
        h.require(&permission, AccessTarget::Operation, &mut ex).await.unwrap();
    }
    assert_eq!(h.calls("sales_inquiry"), 0);
    assert_eq!(h.permission_calls(), 2);

    h.grants(&[("bank-reader", &[party::BANK_ACCOUNT_DETAIL])]);
    assert!(matches!(
        h.require(&payable::PAYMENT_RECIPIENT_REVEAL, AccessTarget::Operation, &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(h.calls("sales_inquiry"), 0);
}

#[tokio::test]
async fn bank_reveal_rejects_non_snapshot_and_record_or_task_targets_before_identity() {
    let h = AuthHarness::new();
    let perm = &payable::PAYMENT_RECIPIENT_REVEAL;
    for ex in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
        assert!(matches!(
            h.require(perm, AccessTarget::Operation, ex).await,
            Err(Error::ValidationError(m)) if m == "写入授权必须使用快照事务"
        ));
    }
    for target in [
        AccessTarget::record("account").unwrap(),
        AccessTarget::Task { id: "task".into(), version: "1".into() },
    ] {
        assert!(matches!(
            h.require_snap(perm, target).await,
            Err(Error::ValidationError(m)) if m == "此权限不支持该授权目标"
        ));
    }
    assert_eq!(h.permission_calls(), 0);
}

// ── 退款与冲正候选提交 ────────────────────────────────────────────────────────

fn funds_origin(resource: &str) -> ObjectFacts {
    ObjectFacts {
        relations: ["sales_source", "purchase_source", "settlement_source"]
            .into_iter()
            .map(|key| (RelationKey::parse(key).unwrap(), RelatedObjects::default()))
            .collect(),
        ..object(resource, "funds", "user")
    }
}

fn commit_candidate(permission: &Permission, key: &str, other: Option<&str>) -> ObjectFacts {
    let mut value = candidate(permission.resource(), "user");
    value.relations.insert(
        RelationKey::parse(key).unwrap(),
        RelatedObjects { expected_ids: ["funds".into()].into(), objects: vec![] },
    );
    if let Some(other) = other {
        value.relations.insert(RelationKey::parse(other).unwrap(), RelatedObjects::default());
    }
    value
}

#[tokio::test]
async fn commit_candidates_require_current_submit_and_full_real_source_without_loading_a_new_root() {
    for (perm, source_perm, key, other) in [
        (
            returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
            receivable::CUSTOMER_RECEIPT_DETAIL,
            "receipt_source",
            Some("receivable_source"),
        ),
        (
            returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            payable::SUPPLIER_PAYMENT_DETAIL,
            "payment_source",
            Some("payable_source"),
        ),
        (
            returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            receivable::CUSTOMER_RECEIPT_DETAIL,
            "receipt_source",
            None,
        ),
        (
            returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
            payable::SUPPLIER_PAYMENT_DETAIL,
            "payment_source",
            None,
        ),
    ] {
        let h = AuthHarness::with_multi_facts(&[
            (source_perm.resource(), vec![funds_origin(source_perm.resource())]),
            (perm.resource(), vec![]),
        ]);
        let funds_reader = [source_perm.clone(), policy_permission::FINANCE_LEDGER_READ];
        h.grants(&[("submitter", std::slice::from_ref(&perm)), ("funds-reader", &funds_reader)]);
        let target = || AccessTarget::Candidate(commit_candidate(&perm, key, other));
        let mut ex = SnapshotExecutor::default();

        h.require(&perm, target(), &mut ex).await.unwrap();
        assert_eq!(h.calls(perm.resource()), 0);
        assert_eq!(h.calls(source_perm.resource()), 1);
        assert!(matches!(h.require_read(&perm, target()).await, Err(Error::ValidationError(_))));

        let calls = h.calls(source_perm.resource());
        h.grants(&[("funds-reader", &funds_reader)]);
        assert!(matches!(h.require(&perm, target(), &mut ex).await, Err(Error::Forbidden(_))));
        assert_eq!(h.calls(source_perm.resource()), calls);

        h.grants(&[("submitter", std::slice::from_ref(&perm)), ("funds-reader", &funds_reader)]);
        h.set_objects(source_perm.resource(), vec![]);
        assert!(matches!(h.require(&perm, target(), &mut ex).await, Err(Error::Forbidden(_))));

        h.set_objects(source_perm.resource(), vec![funds_origin(source_perm.resource())]);
        h.grants(&[("submitter", std::slice::from_ref(&perm))]);
        assert!(matches!(h.require(&perm, target(), &mut ex).await, Err(Error::Forbidden(_))));

        h.grants(&[("submitter", std::slice::from_ref(&perm)), ("funds-reader", &funds_reader)]);
        let mut wrong_handler = commit_candidate(&perm, key, other);
        wrong_handler.owner_id = FactValue::Known(Some("other".into()));
        assert!(matches!(
            h.require(&perm, AccessTarget::Candidate(wrong_handler), &mut ex).await,
            Err(Error::Forbidden(_))
        ));

        let mut fabricated = commit_candidate(&perm, key, other);
        fabricated
            .relations
            .get_mut(&RelationKey::parse(key).unwrap())
            .unwrap()
            .objects
            .push(funds_origin(source_perm.resource()));
        assert!(matches!(
            h.require(&perm, AccessTarget::Candidate(fabricated), &mut ex).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            h.require(&perm, AccessTarget::Operation, &mut ex).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            h.require(&perm, AccessTarget::record("missing").unwrap(), &mut ex).await,
            Err(Error::NotFound(_))
        ));
    }
}

#[tokio::test]
async fn whole_finance_qualification_comes_from_one_role_and_not_from_plan_binding() {
    let h = AuthHarness::with_facts("cost_entry", vec![]);
    let mut ex = SnapshotExecutor::default();
    let entry = [cost::ENTRY_LIST, sales_order::LIST];
    let ledger = [policy_permission::FINANCE_LEDGER_READ];
    for (roles, expected) in [
        (vec![("entry", entry.as_slice())], false),
        (vec![("entry", entry.as_slice()), ("ledger", ledger.as_slice())], false),
    ] {
        h.grants(&roles);
        let access = h.query(&cost::ENTRY_LIST, &mut ex).await.unwrap();
        assert!(access.whole_stages().is_ok());
        assert_eq!(access.has_whole_qualification(), expected);
        h.validate(&access, &mut ex).await.unwrap();
    }
    h.grants(&[("whole", &[cost::ENTRY_LIST, sales_order::LIST, policy_permission::FINANCE_LEDGER_READ])]);
    h.set_denied_scope(true);
    let access = h.query(&cost::ENTRY_LIST, &mut ex).await.unwrap();
    assert!(access.has_whole_qualification());
    h.validate(&access, &mut ex).await.unwrap();
}

// ── 库存仓库与库存调整 ────────────────────────────────────────────────────────

fn warehouse_entries() -> [(Permission, Permission); 8] {
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

fn warehouse_fact(id: &str) -> ObjectFacts {
    ObjectFacts {
        warehouse_id: FactValue::Known(Some(id.into())),
        ..object("inventory_warehouse", id, "other")
    }
}

fn warehouse_rows() -> Vec<Document> {
    vec![
        doc! {"id":"allowed", "warehouse_id":"allowed", "deleted_at":0},
        doc! {"id":"other", "warehouse_id":"other", "deleted_at":0},
        doc! {"id":"deleted", "warehouse_id":"allowed", "deleted_at":1},
    ]
}

fn set_warehouse_scope(h: &AuthHarness, source: &Permission, ids: &[&str]) {
    h.identity.role_scopes.lock().unwrap().clear();
    h.set_role_scope(
        "role",
        source,
        ScopeClause { warehouse_ids: ids.iter().map(|id| (*id).into()).collect(), ..Default::default() },
    );
}

#[tokio::test]
async fn warehouse_entries_reuse_original_inventory_permissions_and_scope_without_granting_internal_codes() {
    let h = AuthHarness::with_facts(
        "inventory_warehouse",
        vec![warehouse_fact("allowed"), warehouse_fact("other")],
    );
    for (entry, source) in warehouse_entries() {
        set_warehouse_scope(&h, &source, &["allowed"]);
        let required = rules::same_role_permissions(&source);
        h.grants(&[("role", &required)]);
        let mut ex = SnapshotExecutor::default();
        h.require(&entry, AccessTarget::record("allowed").unwrap(), &mut ex).await.unwrap();
        assert!(matches!(
            h.require(&entry, AccessTarget::record("other").unwrap(), &mut ex).await,
            Err(Error::Forbidden(_))
        ));

        let access = h.query(&entry, &mut ex).await.unwrap();
        let rows = execute(warehouse_rows(), access.stages(), &BTreeMap::new(), &doc! {});
        assert_eq!(rows.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), ["allowed"]);
        h.validate(&access, &mut ex).await.unwrap();

        let before = h.calls("inventory_warehouse");
        h.grants(&[("role", std::slice::from_ref(&entry))]);
        assert!(matches!(
            h.require(&entry, AccessTarget::record("allowed").unwrap(), &mut ex).await,
            Err(Error::Forbidden(_))
        ));
        assert!(matches!(h.query(&entry, &mut ex).await, Err(Error::Forbidden(_))));
        assert_eq!(h.calls("inventory_warehouse"), before);
    }
}

#[tokio::test]
async fn adjustment_warehouse_checks_preserve_same_role_detail_and_reject_missing_real_warehouses() {
    let h = AuthHarness::with_facts("inventory_warehouse", vec![warehouse_fact("allowed")]);
    for (entry, source) in warehouse_entries().into_iter().filter(|(_, s)| {
        [
            inventory::STOCK_ADJUSTMENT_LIST,
            inventory::STOCK_ADJUSTMENT_CREATE,
            inventory::STOCK_ADJUSTMENT_UPDATE,
        ]
        .contains(s)
    }) {
        set_warehouse_scope(&h, &source, &["allowed", "missing"]);
        let mut ex = SnapshotExecutor::default();
        h.grants(&[
            ("role", std::slice::from_ref(&source)),
            ("reader", &[inventory::STOCK_ADJUSTMENT_DETAIL]),
        ]);
        assert!(matches!(
            h.require(&entry, AccessTarget::record("allowed").unwrap(), &mut ex).await,
            Err(Error::Forbidden(_))
        ));
        assert!(matches!(h.query(&entry, &mut ex).await, Err(Error::Forbidden(_))));

        h.grants(&[("role", &[source, inventory::STOCK_ADJUSTMENT_DETAIL])]);
        h.require(&entry, AccessTarget::record("allowed").unwrap(), &mut ex).await.unwrap();
        assert!(matches!(
            h.require(&entry, AccessTarget::record("missing").unwrap(), &mut ex).await,
            Err(Error::NotFound(_))
        ));
    }
}

#[tokio::test]
async fn warehouse_targets_cannot_be_fabricated_candidates_or_replace_existing_adjustment_objects() {
    let h = AuthHarness::with_facts("inventory_warehouse", vec![warehouse_fact("warehouse")]);
    let entry = internal::INVENTORY_CREATE_WAREHOUSES;
    set_warehouse_scope(&h, &inventory::STOCK_ADJUSTMENT_CREATE, &["warehouse"]);
    let mut ex = SnapshotExecutor::default();
    for target in [AccessTarget::Candidate(warehouse_fact("warehouse")), AccessTarget::Operation] {
        assert!(matches!(h.require(&entry, target, &mut ex).await, Err(Error::ValidationError(_))));
    }
    assert!(matches!(
        h.require(&inventory::STOCK_ADJUSTMENT_DETAIL, AccessTarget::record("warehouse").unwrap(), &mut ex)
            .await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        h.require_read(&entry, AccessTarget::record("warehouse").unwrap()).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn warehouse_existence_is_independent_of_empty_inventory_and_empty_scope_never_becomes_unrestricted() {
    let h = AuthHarness::with_facts("inventory_warehouse", vec![warehouse_fact("allowed")]);
    let entry = internal::INVENTORY_BALANCE_WAREHOUSES;
    let source = inventory::STOCK_BALANCE_LIST;
    h.grants(&[("role", std::slice::from_ref(&source))]);
    for (ids, expected) in [(vec!["allowed"], vec!["allowed"]), (vec![], vec![])] {
        set_warehouse_scope(&h, &source, &ids);
        let mut ex = SnapshotExecutor::default();
        let access = h.query(&entry, &mut ex).await.unwrap();
        let warehouses = execute(warehouse_rows(), access.stages(), &BTreeMap::new(), &doc! {});
        assert_eq!(warehouses.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), expected);
        h.validate(&access, &mut ex).await.unwrap();

        let inv = h.query(&source, &mut ex).await.unwrap();
        assert!(execute(Vec::new(), inv.stages(), &BTreeMap::new(), &doc! {}).is_empty());
        h.validate(&inv, &mut ex).await.unwrap();

        let record = h.require(&entry, AccessTarget::record("allowed").unwrap(), &mut ex).await;
        assert_eq!(record.is_ok(), !ids.is_empty());
    }
}

#[tokio::test]
async fn warehouse_delivery_then_inventory_query_uses_a_new_nontransaction_read_stage() {
    let h = AuthHarness::with_facts("inventory_warehouse", vec![warehouse_fact("allowed")]);
    set_warehouse_scope(&h, &inventory::STOCK_BALANCE_LIST, &["allowed"]);
    let mut ex = ReadExecutor::default();
    let warehouse = h.query(&internal::INVENTORY_BALANCE_WAREHOUSES, &mut ex).await.unwrap();
    h.validate(&warehouse, &mut ex).await.unwrap();

    let inv = h.query(&inventory::STOCK_BALANCE_LIST, &mut ex).await.unwrap();
    assert!(matches!(warehouse.ensure_current(&mut ex), Err(Error::ValidationError(_))));
    h.validate(&inv, &mut ex).await.unwrap();
    ex.next_read();
    assert!(matches!(inv.ensure_current(&mut ex), Err(Error::ValidationError(_))));
}

#[tokio::test]
async fn stock_adjustment_update_checks_the_persisted_record_and_complete_warehouse_source() {
    let perm = &inventory::STOCK_ADJUSTMENT_UPDATE;
    let required = [inventory::STOCK_ADJUSTMENT_UPDATE, inventory::STOCK_ADJUSTMENT_DETAIL];
    let mut adjustment = object("stock_adjustment", "adjustment", "other");
    adjustment.warehouse_id = FactValue::Known(Some("warehouse".into()));
    attach_relation(&mut adjustment, "warehouse", object("warehouse", "warehouse", "other"));

    let h = AuthHarness::with_facts("stock_adjustment", vec![adjustment.clone()]);
    h.set_scope(
        perm,
        perm,
        ResolvedScope {
            role_clauses: vec![ScopeClause {
                warehouse_ids: ["warehouse".into()].into(),
                ..Default::default()
            }],
            user_limit: None,
        },
    );
    let target = || AccessTarget::record("adjustment").unwrap();
    let mut ex = SnapshotExecutor::default();
    h.grants(&[("warehouse-editor", &required)]);
    h.require(perm, target(), &mut ex).await.unwrap();

    for roles in [
        vec![("update-only", &required[..1])],
        vec![("detail-only", &required[1..])],
        vec![("editor", &required[..1]), ("reader", &required[1..])],
    ] {
        h.grants(&roles);
        let calls = h.calls("stock_adjustment");
        assert!(matches!(h.require(perm, target(), &mut ex).await, Err(Error::Forbidden(_))));
        assert_eq!(h.calls("stock_adjustment"), calls);
    }

    h.grants(&[("warehouse-editor", &required)]);
    h.mutate_first("stock_adjustment", |o| o.warehouse_id = FactValue::Known(Some("outside".into())));
    assert!(matches!(h.require(perm, target(), &mut ex).await, Err(Error::Forbidden(_))));

    let mut missing_wh = adjustment.clone();
    missing_wh.relations.get_mut(&RelationKey::parse("warehouse").unwrap()).unwrap().objects.clear();
    h.set_objects("stock_adjustment", vec![missing_wh]);
    assert!(matches!(h.require(perm, target(), &mut ex).await, Err(Error::Forbidden(_))));

    let mut unknown_wh = adjustment;
    unknown_wh.warehouse_id = FactValue::Missing;
    h.set_objects("stock_adjustment", vec![unknown_wh]);
    assert!(
        matches!(h.require(perm, target(), &mut ex).await, Err(Error::ValidationError(m)) if m.contains("所属仓库"))
    );

    assert!(matches!(
        h.require(perm, AccessTarget::record("missing").unwrap(), &mut ex).await,
        Err(Error::NotFound(_))
    ));
    let calls = h.calls("stock_adjustment");
    assert!(matches!(
        h.require(perm, AccessTarget::Candidate(candidate("stock_adjustment", "user")), &mut ex).await,
        Err(Error::ValidationError(m)) if m == "此权限不支持该授权目标"
    ));
    assert_eq!(h.calls("stock_adjustment"), calls);
}
