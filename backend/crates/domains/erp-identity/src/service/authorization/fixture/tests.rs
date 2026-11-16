use std::sync::atomic::Ordering;

use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_permissions::{admin, sales_inquiry, sales_order};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{NoTransaction, ReadExecutor};

use super::*;
use crate::AccessTarget;
use crate::entity::access_control::ScopeClause;
use crate::repository::authorization::testing::mapping;

fn actor() -> AuditActor {
    AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin)
}

fn service(scope: ResolvedScope) -> Arc<AuthorizationService> {
    authorization(
        actor(),
        vec![admin::ROLE_CREATE_ROLE, sales_inquiry::QUERY_SALES_LIST],
        scope,
        BTreeMap::new(),
        [(
            sales_inquiry::QUERY_SALES_LIST,
            mapping(&sales_inquiry::QUERY_SALES_LIST) as Arc<dyn AccessMapping>,
        )]
        .into(),
    )
    .unwrap()
}

fn scope() -> ResolvedScope {
    ResolvedScope {
        role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
        user_limit: None,
    }
}

#[tokio::test]
async fn real_operation_distinguishes_permission_identity_and_snapshot() {
    let service = service(scope());
    let mut executor = SnapshotExecutor::default();
    service
        .require(&actor(), &admin::ROLE_CREATE_ROLE, AccessTarget::Operation, &mut executor)
        .await
        .unwrap();
    assert!(matches!(
        service.require(&actor(), &admin::ROLE_DELETE_ROLE, AccessTarget::Operation, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    let other = AuditActor::new("other".into(), "other".into(), AccountKind::Admin);
    assert!(matches!(
        service.require(&other, &admin::ROLE_CREATE_ROLE, AccessTarget::Operation, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &admin::ROLE_CREATE_ROLE,
                AccessTarget::Operation,
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut NoTransaction).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        AuthorizationService::unavailable_for_test()
            .require(&actor(), &admin::ROLE_CREATE_ROLE, AccessTarget::Operation, &mut executor)
            .await,
        Err(Error::Internal(_))
    ));
}

#[tokio::test]
async fn query_issues_a_real_plan_and_preserves_all_delivery_boundaries() {
    let service = service(scope());
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut executor).await.unwrap();
    assert_eq!(access.actor().id(), actor().id());
    assert!(!access.stages().is_empty());
    assert!(access.has_role_scope());
    service.validate(&access, &mut executor).await.unwrap();
    assert!(matches!(
        service.validate(&access, &mut SnapshotExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    executor.next_read();
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ValidationError(_))));
    let access = service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut executor).await.unwrap();
    executor.restart();
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ValidationError(_))));
}

#[tokio::test]
async fn positive_scope_summary_cannot_be_created_by_a_personal_limit() {
    let service = service(ResolvedScope {
        role_clauses: vec![],
        user_limit: Some(ScopeClause { company: true, ..Default::default() }),
    });
    let mut executor = ReadExecutor::default();
    let access = service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut executor).await.unwrap();
    assert!(!access.has_role_scope());
    service.validate(&access, &mut executor).await.unwrap();
}

#[tokio::test]
async fn real_query_preserves_expiry_and_dynamic_dependency_validation() {
    let mut mapping = mapping(&sales_inquiry::QUERY_SALES_LIST);
    Arc::get_mut(&mut mapping).unwrap().dynamic = true;
    let service = authorization(
        actor(),
        vec![sales_inquiry::QUERY_SALES_LIST],
        scope(),
        BTreeMap::new(),
        [(sales_inquiry::QUERY_SALES_LIST, mapping.clone() as Arc<dyn AccessMapping>)].into(),
    )
    .unwrap();
    let mut executor = ReadExecutor::default();
    *mapping.expires.lock().unwrap() = Some(Instant::from_unix_secs(1));
    assert!(matches!(
        service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut executor).await,
        Err(Error::ConflictError(_))
    ));
    *mapping.expires.lock().unwrap() = None;
    let access = service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut executor).await.unwrap();
    mapping.revision.fetch_add(1, Ordering::Relaxed);
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
}

#[tokio::test]
async fn unbound_queries_and_objects_remain_closed_with_a_valid_test_identity() {
    let service = authorization(
        actor(),
        vec![sales_inquiry::QUERY_SALES_DETAIL],
        scope(),
        BTreeMap::new(),
        BTreeMap::new(),
    )
    .unwrap();
    let mut executor = SnapshotExecutor::default();
    assert!(matches!(
        service.query(&actor(), &sales_inquiry::QUERY_SALES_DETAIL, &mut executor).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &sales_inquiry::QUERY_SALES_DETAIL,
                AccessTarget::record("object").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[test]
fn fixture_rejects_unknown_permissions_and_mismatched_bindings() {
    let create =
        |permissions, mappings| authorization(actor(), permissions, scope(), BTreeMap::new(), mappings);
    assert!(create(vec![Permission::new("*", "*")], BTreeMap::new()).is_err());
    assert!(create(vec![Permission::new("unknown", "read")], BTreeMap::new()).is_err());
    assert!(
        create(
            vec![],
            [(sales_inquiry::QUERY_SALES_LIST, mapping(&sales_order::DETAIL) as Arc<dyn AccessMapping>)]
                .into()
        )
        .is_err()
    );
}
