//! 用替身设施执行真实 Builder 和公共 API；不连接数据库。
mod access_inspection;
mod approval_cancel;
mod approval_decision;
mod approval_management;
mod bank_reveal;
mod contract_import;
mod entry_migration;
mod fixtures;
mod integration_difference;
mod inventory_warehouses;
mod management_contract;
mod migration_fixes;
mod phase5;
mod phase5_regressions;
mod phase5_review_fixes;
mod phase6_read_entries;
mod policy_bundles;
mod purchase_entries;
mod read_delivery_expiry;
mod reverse_commit;
mod review_handover_delivery;
mod review_management_query;
mod review_query_targets;
mod sales_change_draft;
mod supplier_offering_detail;
mod work_item_detail;
mod work_item_execution;
mod work_item_qualifications;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::{
    approval_instance, integration_ops, purchase_payment_request, receivable, returns,
    sales_inquiry as inquiry, sales_order,
};
use fixtures::*;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};
use serde_json::json;

use super::*;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessTarget, FactKind, FactValue, ObjectFacts, QueryOutput, RelatedObjects, RelationKey,
};
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::testing::{receipt_projection, task_members};
use crate::{Error, ResourceTargets};

#[test]
fn builder_accepts_complete_bindings_and_rejects_missing_duplicate_unknown_and_mismatched_bindings() {
    complete().build().unwrap();
    assert!(
        matches!(empty().build(), Err(Error::ValidationError(message)) if message.contains("缺少资源 Facts"))
    );
    let mut missing = complete();
    missing.mappings.remove(&inquiry::QUERY_SALES_LIST);
    assert!(
        matches!(missing.build(), Err(Error::ValidationError(message)) if message.contains("缺少查询 Mapping"))
    );

    let facts = Arc::new(Facts::default());
    assert!(matches!(empty().facts("unknown", facts.clone()), Err(Error::ValidationError(_))));
    assert!(matches!(
        empty().facts("sales_inquiry", facts.clone()).unwrap().facts("sales_inquiry", facts),
        Err(Error::ValidationError(_))
    ));

    let map = mapping(&inquiry::QUERY_SALES_LIST);
    assert!(matches!(
        empty()
            .mapping(&inquiry::QUERY_SALES_LIST, map.clone())
            .unwrap()
            .mapping(&inquiry::QUERY_SALES_LIST, map),
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        empty().mapping(&inquiry::COMMANDS_SALES_SAVE, mapping(&inquiry::QUERY_SALES_LIST)),
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        empty().mapping(&Permission::new("unwired", "list"), mapping(&inquiry::QUERY_SALES_LIST)),
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        empty().mapping(&sales_order::LIST, mapping(&inquiry::QUERY_SALES_LIST)),
        Err(Error::ValidationError(_))
    ));
}

#[test]
fn builder_checks_fact_capabilities_sources_and_output_before_any_request() {
    let permission = &inquiry::QUERY_SALES_LIST;
    let mut map = mapping(permission);
    Arc::get_mut(&mut map).unwrap().model.facts.remove(&FactKind::Organization);
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map);
    assert!(matches!(builder.build(), Err(Error::ValidationError(_))));

    let mut map = mapping(permission);
    Arc::get_mut(&mut map).unwrap().model.output = QueryOutput::Share { fields: ["id".into()].into() };
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map);
    assert!(matches!(builder.build(), Err(Error::ValidationError(_))));

    let permission = &receivable::CUSTOMER_RECEIPT_LIST;
    let mut map = mapping(permission);
    Arc::get_mut(&mut map).unwrap().relations.clear();
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map);
    assert!(matches!(builder.build(), Err(Error::ValidationError(_))));
}

#[tokio::test]
async fn require_checks_real_owner_and_missing_facts_and_stops_before_facts_without_qualification() {
    let identity = Arc::new(Identity::default());
    let port = Arc::new(Facts::new(vec![object("sales_inquiry", "one", "user")]));
    let service = service(identity.clone(), port.clone());
    let mut ex = SnapshotExecutor::default();
    service
        .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
        .await
        .unwrap();
    assert_eq!(port.calls.load(Ordering::SeqCst), 1);
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 1);

    port.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
            .await,
        Err(Error::Forbidden(_))
    ));

    port.objects.lock().unwrap()[0].owner_id = FactValue::Missing;
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
            .await,
        Err(Error::ValidationError(_))
    ));

    identity.state.lock().unwrap().grants = vec![];
    let calls = port.calls.load(Ordering::SeqCst);
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::record("one").unwrap(), &mut ex)
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(port.calls.load(Ordering::SeqCst), calls);

    assert!(matches!(
        service.require(&actor(), &inquiry::COMMANDS_SALES_SAVE, AccessTarget::Operation, &mut ex).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &inquiry::COMMANDS_SALES_SAVE,
                AccessTarget::record("one").unwrap(),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn candidates_and_ownership_change_check_server_destination_and_original() {
    let port = Arc::new(Facts::new(vec![object("sales_inquiry", "one", "user")]));
    let service = service(Arc::new(Identity::default()), port.clone());
    let mut ex = SnapshotExecutor::default();
    let destination = candidate("sales_inquiry", "user");
    service
        .require(
            &actor(),
            &inquiry::COMMANDS_SALES_CREATE,
            AccessTarget::Candidate(destination.clone()),
            &mut ex,
        )
        .await
        .unwrap();
    assert_eq!(port.calls.load(Ordering::SeqCst), 0);

    for (owner, allowed) in [("user", true), ("other", false)] {
        let target =
            AccessTarget::OwnershipChange { id: "one".into(), candidate: candidate("sales_inquiry", owner) };
        let result = service.require(&actor(), &inquiry::COMMANDS_SALES_SAVE, target, &mut ex).await;
        if allowed {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }

    let mut forged = destination;
    forged.version = "persisted".into();
    assert!(matches!(
        service
            .require(&actor(), &inquiry::COMMANDS_SALES_CREATE, AccessTarget::Candidate(forged), &mut ex)
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn exact_tasks_require_version_assignment_and_same_role_extra_permission() {
    let task = ObjectFacts {
        assignee_id: FactValue::Known(Some("user".into())),
        subject_type: FactValue::Known("stock_adjustment".into()),
        ..object("approval_instance", "task", "user")
    };
    let port = Arc::new(Facts::new(vec![task]));
    let identity = Arc::new(Identity::default());
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("approval_instance".into(), port.clone());
    let service = builder.build().unwrap();
    let mut ex = SnapshotExecutor::default();
    let target = || AccessTarget::Task { id: "task".into(), version: "v1".into() };

    service.require(&actor(), &approval_instance::SUBMIT_DECISION, target(), &mut ex).await.unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::SUBMIT_DECISION,
                AccessTarget::record("task").unwrap(),
                &mut ex
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &approval_instance::SUBMIT_DECISION,
                AccessTarget::Task { id: "task".into(), version: "old".into() },
                &mut ex
            )
            .await,
        Err(Error::ConflictError(_))
    ));

    port.objects.lock().unwrap()[0].assignee_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), &approval_instance::SUBMIT_DECISION, target(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));

    identity.state.lock().unwrap().grants = vec![
        ("decide".into(), vec![approval_instance::SUBMIT_DECISION]),
        ("read".into(), vec![approval_instance::INSTANCE_LIST]),
    ];
    assert!(matches!(
        service.require(&actor(), &approval_instance::SUBMIT_DECISION, target(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn query_compiles_owner_condition_and_validate_respects_snapshot_and_read_dependencies() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert_eq!(inquiry_members(&access), vec!["one"]);
    let calls = identity.permissions_calls.load(Ordering::SeqCst);
    service.validate(&access, &mut ex).await.unwrap();
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), calls);
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));

    let mut ex = ReadExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    service.validate(&access, &mut ex).await.unwrap();

    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    identity.revision.fetch_add(1, Ordering::SeqCst);
    assert!(
        matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
    );

    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    identity.state.lock().unwrap().active = false;
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));
}

#[tokio::test]
async fn functional_queries_preserve_disabled_empty_and_independent_grants() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = ReadExecutor::default();
    identity.state.lock().unwrap().grants = vec![
        ("read".into(), vec![inquiry::QUERY_SALES_LIST]),
        ("write".into(), vec![inquiry::COMMANDS_SALES_SAVE]),
    ];
    let permissions = [inquiry::QUERY_SALES_LIST, inquiry::COMMANDS_SALES_SAVE];
    let qualified = service.permissions(&actor(), &permissions, &mut ex).await.unwrap().unwrap();
    assert_eq!(
        qualified.permissions().into_vec().into_iter().collect::<BTreeSet<_>>(),
        permissions.clone().into()
    );
    assert!(!qualified.same_role(&permissions));
    identity.state.lock().unwrap().grants = vec![];
    assert!(
        service
            .permissions(&actor(), &permissions, &mut ex)
            .await
            .unwrap()
            .unwrap()
            .permissions()
            .into_vec()
            .is_empty()
    );
    assert!(!service.has_permission(&actor(), &permissions[0], &mut ex).await.unwrap());
    identity.state.lock().unwrap().active = false;
    assert!(service.permissions(&actor(), &permissions, &mut ex).await.unwrap().is_none());
}

#[tokio::test]
async fn query_denies_empty_scope_and_propagates_evidence_failures() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = ReadExecutor::default();
    identity.state.lock().unwrap().denied_scope = true;
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert!(inquiry_members(&access).is_empty());
    identity.state.lock().unwrap().fail = true;
    assert!(
        matches!(service.validate(&access, &mut ex).await, Err(Error::Internal(message)) if message == "identity storage unavailable")
    );
    assert!(matches!(
        service.has_permission(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await,
        Err(Error::Internal(_))
    ));
}

#[tokio::test]
async fn credentials_reject_other_service_executor_next_query_restart_and_raw_session() {
    let service = service(Arc::new(Identity::default()), Arc::new(Facts::default()));
    let other = complete().build().unwrap();
    let mut ex = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert!(matches!(other.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
    assert!(matches!(
        service.validate(&access, &mut SnapshotExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    let newer = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
    ex.restart();
    assert!(matches!(service.validate(&newer, &mut ex).await, Err(Error::ValidationError(_))));
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    ex.session();
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ValidationError(_))));
}

#[tokio::test]
async fn dynamic_mapping_revisions_and_expiry_are_checked_by_public_delivery_api() {
    let mut map = mapping(&inquiry::QUERY_SALES_LIST);
    Arc::get_mut(&mut map).unwrap().dynamic = true;
    let mut builder = complete();
    builder.mappings.insert(inquiry::QUERY_SALES_LIST, map.clone());
    let service = builder.build().unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), 0);

    let mut ex = ReadExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), 1);
    service.validate(&access, &mut ex).await.unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), 2);

    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    map.revision.fetch_add(1, Ordering::SeqCst);
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));

    let mut snapshot = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut snapshot).await.unwrap();
    let calls = map.prepares.load(Ordering::SeqCst);
    service.validate(&access, &mut snapshot).await.unwrap();
    assert_eq!(map.prepares.load(Ordering::SeqCst), calls);

    *map.expires.lock().unwrap() = Some(Instant::from_unix_secs(1));
    assert!(matches!(
        service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut snapshot).await,
        Err(Error::ConflictError(_))
    ));
}

#[tokio::test]
async fn dynamic_mapping_without_revision_only_supports_snapshot_and_cannot_change_stage() {
    let mut map = mapping(&inquiry::QUERY_SALES_LIST);
    let data = Arc::get_mut(&mut map).unwrap();
    data.dynamic = true;
    data.reliable = false;
    let mut builder = complete();
    builder.mappings.insert(inquiry::QUERY_SALES_LIST, map.clone());
    let service = builder.build().unwrap();
    assert!(matches!(
        service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    let mut ex = SnapshotExecutor::default();
    let access = service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await.unwrap();
    service.validate(&access, &mut ex).await.unwrap();
    map.change_stage.store(true, Ordering::SeqCst);
    assert!(matches!(
        service.query(&actor(), &inquiry::QUERY_SALES_LIST, &mut ex).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn whole_record_reads_require_functional_permission_and_revalidate_grant_revision() {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().denied_scope = true;
    identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "one".into()));
    let port = Arc::new(Facts::new(vec![object("sales_order", "one", "other")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("sales_order".into(), port);
    let service = builder.build().unwrap();
    let mut ex = ReadExecutor::default();
    service
        .require(&actor(), &sales_order::DETAIL, AccessTarget::record("one").unwrap(), &mut ex)
        .await
        .unwrap();

    let access = service.query(&actor(), &sales_order::LIST, &mut ex).await.unwrap();
    identity.grant_revision.fetch_add(1, Ordering::SeqCst);
    assert!(matches!(service.validate(&access, &mut ex).await, Err(Error::ConflictError(_))));

    identity.state.lock().unwrap().grants.clear();
    assert!(matches!(
        service.require(&actor(), &sales_order::DETAIL, AccessTarget::record("one").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn source_creation_loads_actual_references_and_rejects_missing_or_fabricated_sources() {
    let identity = Arc::new(Identity::default());
    let permission = purchase_payment_request::CREATE;
    identity.state.lock().unwrap().grants = vec![("entry".into(), vec![permission.clone()])];
    let port = Arc::new(Facts::new(vec![object("purchase_order", "purchase", "user")]));
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("purchase_order".into(), port.clone());
    let service = builder.build().unwrap();
    let candidate = ObjectFacts {
        relations: [(
            RelationKey::parse("purchase_source").unwrap(),
            RelatedObjects { expected_ids: ["purchase".into()].into(), objects: vec![] },
        )]
        .into(),
        ..candidate(permission.resource(), "user")
    };
    let mut ex = SnapshotExecutor::default();

    service
        .require(&actor(), &permission, AccessTarget::Candidate(candidate.clone()), &mut ex)
        .await
        .unwrap();

    port.objects.lock().unwrap().clear();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Candidate(candidate.clone()), &mut ex).await,
        Err(Error::Forbidden(_))
    ));

    let mut fabricated = candidate;
    fabricated.relations.values_mut().next().unwrap().objects.push(object(
        "purchase_order",
        "purchase",
        "user",
    ));
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Candidate(fabricated), &mut ex).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn independent_refund_origins_keep_their_own_scope_roles() {
    let permission = returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT;
    let receipt = receivable::CUSTOMER_RECEIPT_DETAIL;
    let receivable = receivable::ACCOUNT_DETAIL;
    let identity = Arc::new(Identity::default());
    identity
        .scopes
        .lock()
        .unwrap()
        .insert((receipt, sales_order::DETAIL), ResolvedScope { role_clauses: vec![], user_limit: None });
    identity.scopes.lock().unwrap().insert(
        (receivable.clone(), sales_order::DETAIL),
        ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        },
    );
    let origin = ObjectFacts {
        relations: [(
            RelationKey::parse("sales_source").unwrap(),
            RelatedObjects {
                expected_ids: ["sale".into()].into(),
                objects: vec![object("sales_order", "sale", "user")],
            },
        )]
        .into(),
        ..object(receivable.resource(), "account", "user")
    };
    let refund = ObjectFacts {
        relations: [
            (RelationKey::parse("receipt_source").unwrap(), RelatedObjects::default()),
            (
                RelationKey::parse("receivable_source").unwrap(),
                RelatedObjects { expected_ids: ["account".into()].into(), objects: vec![origin] },
            ),
        ]
        .into(),
        ..object(permission.resource(), "refund", "user")
    };
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert(permission.resource().into(), Arc::new(Facts::new(vec![refund])));
    let service = builder.build().unwrap();
    let mut ex = SnapshotExecutor::default();
    service.require(&actor(), &permission, AccessTarget::record("refund").unwrap(), &mut ex).await.unwrap();

    identity.state.lock().unwrap().grants = vec![("entry".into(), vec![permission.clone()])];
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::record("refund").unwrap(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn startup_source_bindings_cannot_be_replaced_at_query_time() {
    let permission = receivable::ACCOUNT_LIST;
    let map = mapping(&permission);
    let mut builder = complete();
    builder.mappings.insert(permission.clone(), map.clone());
    let service = builder.build().unwrap();
    map.replace_source.store(true, Ordering::SeqCst);
    assert!(matches!(
        service.query(&actor(), &permission, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn operation_only_permissions_use_rbac_and_cannot_be_replaced_with_record_targets() {
    let permission = integration_ops::ERROR_TASK_CREATE;
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut ex = SnapshotExecutor::default();
    service.require(&actor(), &permission, AccessTarget::Operation, &mut ex).await.unwrap();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::record("record").unwrap(), &mut ex).await,
        Err(Error::ValidationError(_))
    ));
    identity.state.lock().unwrap().grants.clear();
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Operation, &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

fn payment_sources() -> std::collections::BTreeMap<RelationKey, RelatedObjects> {
    [(
        RelationKey::parse("purchase_source").unwrap(),
        RelatedObjects {
            expected_ids: ["purchase".into()].into(),
            objects: vec![object("purchase_order", "purchase", "user")],
        },
    )]
    .into()
}

#[tokio::test]
async fn payment_requires_detail_on_the_same_role_for_tasks_and_queries() {
    let permission = purchase_payment_request::CREDIT_LIST;
    let detail = purchase_payment_request::DETAIL;
    let identity = Arc::new(Identity::default());
    let task = ObjectFacts {
        relations: payment_sources(),
        assignee_id: FactValue::Known(Some("user".into())),
        ..object(permission.resource(), "task", "user")
    };
    let port = Arc::new(Facts::new(vec![task]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert(rules::task_facts_resource(&permission).into(), port.clone());
    let service = builder.build().unwrap();
    let target = || AccessTarget::Task { id: "task".into(), version: "v1".into() };
    let mut ex = SnapshotExecutor::default();
    for (grants, allowed) in [
        (vec![("cashier".into(), vec![permission.clone()])], false),
        (vec![("cashier".into(), vec![permission.clone()]), ("reader".into(), vec![detail.clone()])], false),
        (vec![("cashier".into(), vec![permission.clone(), detail])], true),
    ] {
        identity.state.lock().unwrap().grants = grants;
        let before = port.calls.load(Ordering::SeqCst);
        let result = service.require(&actor(), &permission, target(), &mut ex).await;
        let query = service.query(&actor(), &permission, &mut ex).await;
        if allowed {
            result.unwrap();
            service.validate(&query.unwrap(), &mut ex).await.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
            assert!(matches!(query, Err(Error::Forbidden(_))));
            assert_eq!(port.calls.load(Ordering::SeqCst), before);
        }
    }
    port.objects.lock().unwrap()[0].assignee_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), &permission, target(), &mut ex).await,
        Err(Error::Forbidden(_))
    ));
}

#[test]
fn existing_read_permissions_keep_their_query_bindings() {
    for permission in super::rules::queries::PERMISSIONS {
        empty().mapping(permission, mapping(permission)).unwrap();
    }
}

#[tokio::test]
async fn existence_bindings_reject_payload_and_only_deliver_the_row_identity() {
    let permission = receivable::CUSTOMER_RECEIPT_LIST;
    for field in ["total_amount", "amount", "payload", "source.customer_name"] {
        let mut map = mapping(&permission);
        Arc::get_mut(&mut map).unwrap().model.output =
            QueryOutput::Existence { fields: ["id".into(), field.into()].into() };
        assert!(matches!(empty().mapping(&permission, map.clone()), Err(Error::ValidationError(_))));
        let mut builder = complete();
        builder.mappings.insert(permission.clone(), map);
        assert!(matches!(builder.build(), Err(Error::ValidationError(_))));
    }

    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants = vec![("viewer".into(), vec![permission.clone()])];
    let mut map = mapping(&permission);
    Arc::get_mut(&mut map).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let mut builder = complete();
    builder.facilities = identity;
    builder.mappings.remove(&permission);
    let service = builder.mapping(&permission, map).unwrap().build().unwrap();
    let mut ex = ReadExecutor::default();
    let access = service.query(&actor(), &permission, &mut ex).await.unwrap();
    let rows = receipt_projection(&access);
    service.validate(&access, &mut ex).await.unwrap();
    assert_eq!(rows, json!([{"id": "receipt"}]));
}

#[tokio::test]
async fn shared_write_permissions_support_queries_without_relaxing_task_or_transaction_checks() {
    for permission in [inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES, purchase_payment_request::CREDIT_LIST] {
        let mut builder = complete();
        builder.mappings.remove(&permission);
        let samples = [
            ("open", "user", false, "other"),
            ("completed", "user", true, "other"),
            ("participated", "user", true, "user"),
            ("unassigned", "other", false, "user"),
        ];
        let objects = samples
            .iter()
            .map(|(id, assignee, completed, participant)| ObjectFacts {
                assignee_id: FactValue::Known(Some((*assignee).into())),
                completed: FactValue::Known(*completed),
                participants: FactValue::Known([(*participant).into()].into()),
                relations: if permission == purchase_payment_request::CREDIT_LIST {
                    payment_sources()
                } else {
                    Default::default()
                },
                ..object(permission.resource(), id, "user")
            })
            .collect();
        builder.facts.insert(rules::task_facts_resource(&permission).into(), Arc::new(Facts::new(objects)));
        let service = builder.mapping(&permission, mapping(&permission)).unwrap().build().unwrap();

        let mut ex = ReadExecutor::default();
        let access = service.query(&actor(), &permission, &mut ex).await.unwrap();
        let members = task_members(&access, &samples);
        let expected = if permission == inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES {
            vec!["open", "participated"]
        } else {
            vec!["open", "completed", "participated"]
        };
        assert_eq!(members, expected);
        service.validate(&access, &mut ex).await.unwrap();

        for (id, ..) in samples {
            let target = || AccessTarget::Task { id: id.into(), version: "v1".into() };
            let result =
                service.require(&actor(), &permission, target(), &mut SnapshotExecutor::default()).await;
            if expected.contains(&id) {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
            assert!(matches!(
                service.require(&actor(), &permission, target(), &mut ex).await,
                Err(Error::ValidationError(_))
            ));
        }
    }
}

#[test]
fn internal_quote_query_requires_completion_and_participation_mapping() {
    let permission = inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES;
    for fact in [FactKind::Completed, FactKind::Participants] {
        let mut map = mapping(&permission);
        Arc::get_mut(&mut map).unwrap().model.facts.remove(&fact);
        let mut builder = complete();
        builder.mappings.remove(&permission);
        assert!(matches!(builder.mapping(&permission, map).unwrap().build(), Err(Error::ValidationError(_))));
    }
}

#[tokio::test]
async fn internal_quote_task_rejects_missing_completion_or_participation_facts() {
    let permission = inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES;
    let task = ObjectFacts {
        assignee_id: FactValue::Known(Some("user".into())),
        completed: FactValue::Known(false),
        participants: FactValue::Known(BTreeSet::new()),
        ..object(permission.resource(), "task", "user")
    };
    let mut no_completion = task.clone();
    no_completion.completed = FactValue::Missing;
    let mut no_participation = task;
    no_participation.participants = FactValue::Missing;
    for task in [no_completion, no_participation] {
        let mut builder = complete();
        builder
            .facts
            .insert(rules::task_facts_resource(&permission).into(), Arc::new(Facts::new(vec![task])));
        let service = builder.build().unwrap();
        let result = service
            .require(
                &actor(),
                &permission,
                AccessTarget::Task { id: "task".into(), version: "v1".into() },
                &mut SnapshotExecutor::default(),
            )
            .await;
        assert!(matches!(result, Err(Error::ValidationError(_))));
    }
}

struct ReloadFacts {
    first: Vec<ObjectFacts>,
    second: Vec<ObjectFacts>,
    calls: AtomicUsize,
}

#[async_trait]
impl AccessFactsPort for ReloadFacts {
    async fn load(
        &self,
        _: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        Ok(if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.first.clone()
        } else {
            self.second.clone()
        })
    }
}

fn payment_requests() -> Vec<ObjectFacts> {
    ["request_a", "request_b"]
        .into_iter()
        .map(|id| ObjectFacts {
            relations: [(
                RelationKey::parse("purchase_source").unwrap(),
                RelatedObjects {
                    expected_ids: ["purchase_a".into(), "purchase_b".into()].into(),
                    objects: vec![
                        object("purchase_order", "purchase_a", "user"),
                        object("purchase_order", "purchase_b", "user"),
                    ],
                },
            )]
            .into(),
            ..object("purchase_payment_request", id, "user")
        })
        .collect()
}

async fn reloaded_requests(second: Vec<ObjectFacts>) -> Result<()> {
    let port = Arc::new(ReloadFacts { first: payment_requests(), second, calls: AtomicUsize::new(0) });
    let mut builder = complete();
    builder.facts.insert("purchase_payment_request".into(), port);
    let service = builder.build().unwrap();
    service
        .require(
            &actor(),
            &purchase_payment_request::DETAIL,
            AccessTarget::Records(ResourceTargets::new(&["request_a", "request_b"]).unwrap()),
            &mut ReadExecutor::default(),
        )
        .await
}

#[tokio::test]
async fn nontransaction_revalidation_ignores_root_and_source_order() {
    for (roots, sources) in [(true, false), (false, true), (true, true)] {
        let mut second = payment_requests();
        if roots {
            second.reverse();
        }
        if sources {
            for root in &mut second {
                for relation in root.relations.values_mut() {
                    relation.objects.reverse();
                }
            }
        }
        reloaded_requests(second).await.unwrap();
    }
}

#[tokio::test]
async fn nontransaction_revalidation_still_rejects_changed_facts_and_invalid_sets() {
    let mut version = payment_requests();
    version[0].version = "v2".into();
    let mut source_version = payment_requests();
    source_version[0].relations.values_mut().next().unwrap().objects[0].version = "v2".into();
    let mut owner = payment_requests();
    owner[0].relations.values_mut().next().unwrap().objects[0].owner_id =
        FactValue::Known(Some("other".into()));
    let mut references = payment_requests();
    let relation = references[0].relations.values_mut().next().unwrap();
    relation.expected_ids.remove("purchase_b");
    relation.objects.pop();

    for changed in [version, source_version, owner, references] {
        assert!(matches!(reloaded_requests(changed).await, Err(Error::ConflictError(_))));
    }

    let mut duplicate = payment_requests();
    let relation = duplicate[0].relations.values_mut().next().unwrap();
    relation.objects.push(relation.objects[0].clone());
    assert!(matches!(reloaded_requests(duplicate).await, Err(Error::ValidationError(_))));
    let mut duplicate = payment_requests();
    duplicate[1] = duplicate[0].clone();
    assert!(matches!(reloaded_requests(duplicate).await, Err(Error::NotFound(_))));
}
