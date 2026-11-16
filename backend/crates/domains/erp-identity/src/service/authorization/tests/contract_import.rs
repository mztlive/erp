//! 合同识别功能资格不替代归档候选检查；供应商下单沿用完整对象规则。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{contract, customer, internal, supplier_fulfillment};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, candidate, complete, object, service};
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{FactValue, RelatedObjects, RelationKey};
use crate::{AccessTarget, Error};

#[tokio::test]
async fn creation_preparation_reuses_complete_independent_qualification_without_broadening_import() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::default());
    let service = service(identity.clone(), facts.clone());
    let mut executor = SnapshotExecutor::default();
    for grants in
        [vec![], vec![contract::CREATE], vec![customer::DETAIL], vec![internal::CONTRACT_CREATE_PREPARE]]
    {
        identity.state.lock().unwrap().grants = vec![("role".into(), grants)];
        assert!(matches!(
            service
                .require(&actor(), &internal::CONTRACT_CREATE_PREPARE, AccessTarget::Operation, &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
    }
    identity.state.lock().unwrap().grants =
        vec![("creator".into(), vec![contract::CREATE]), ("reader".into(), vec![customer::DETAIL])];
    service
        .require(&actor(), &internal::CONTRACT_CREATE_PREPARE, AccessTarget::Operation, &mut executor)
        .await
        .unwrap();
    for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
        assert!(matches!(
            service
                .require(&actor(), &internal::CONTRACT_CREATE_PREPARE, AccessTarget::Operation, executor)
                .await,
            Err(Error::ValidationError(_))
        ));
    }
    identity.state.lock().unwrap().grants = vec![("creator".into(), vec![contract::CREATE])];
    service
        .require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor)
        .await
        .unwrap();
    assert!(matches!(
        service.require(&actor(), &contract::CREATE, AccessTarget::Operation, &mut executor).await,
        Err(Error::ValidationError(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn contract_creation_uses_distinct_qualified_roles_for_target_and_current_customer_scope() {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants =
        vec![("creator".into(), vec![contract::CREATE]), ("reader".into(), vec![customer::DETAIL])];
    for (role, permission, org) in
        [("creator", customer::LIST, "actor-org"), ("reader", customer::DETAIL, "customer-org")]
    {
        identity.role_scopes.lock().unwrap().insert(
            (role.into(), permission),
            ScopeClause { org_unit_ids: [org.into()].into(), ..Default::default() },
        );
    }
    let mut source = object("customer", "customer", "other");
    source.owner_id = FactValue::Missing;
    source.owner_ids = FactValue::Known(["other".into(), "another".into()].into());
    source.org_unit_id = FactValue::Missing;
    source.org_unit_ids = FactValue::Known(["customer-org".into()].into());
    let mut target = candidate("contract", "user");
    target.org_unit_id = FactValue::Known(Some("actor-org".into()));
    target.relations.insert(
        RelationKey::parse("customer").unwrap(),
        RelatedObjects { expected_ids: ["customer".into()].into(), objects: vec![] },
    );
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("customer".into(), Arc::new(Facts::new(vec![source])));
    let service = builder.build().unwrap();
    service
        .require(
            &actor(),
            &contract::CREATE,
            AccessTarget::Candidate(target.clone()),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    identity
        .role_scopes
        .lock()
        .unwrap()
        .get_mut(&("reader".into(), customer::DETAIL))
        .unwrap()
        .org_unit_ids = ["actor-org".into()].into();
    assert!(matches!(
        service
            .require(
                &actor(),
                &contract::CREATE,
                AccessTarget::Candidate(target),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn import_tasks_require_current_contract_creation_qualification_without_business_fact_reads() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::default());
    let service = service(identity.clone(), facts.clone());
    let mut executor = SnapshotExecutor::default();
    for grants in [vec![], vec![contract::DETAIL], vec![internal::CONTRACT_IMPORT]] {
        identity.state.lock().unwrap().grants = vec![("role".into(), grants)];
        assert!(matches!(
            service
                .require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
    }
    identity.state.lock().unwrap().grants = vec![("role".into(), vec![contract::CREATE])];
    service
        .require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor)
        .await
        .unwrap();
    identity.state.lock().unwrap().active = false;
    assert!(matches!(
        service.require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    identity.state.lock().unwrap().active = true;
    identity.state.lock().unwrap().fail = true;
    assert!(matches!(
        service.require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, &mut executor).await,
        Err(Error::Internal(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn import_entry_rejects_non_snapshot_and_cannot_replace_contract_candidate_authorization() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    identity.state.lock().unwrap().grants = vec![("role".into(), vec![contract::CREATE])];
    for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
        assert!(matches!(
            service.require(&actor(), &internal::CONTRACT_IMPORT, AccessTarget::Operation, executor).await,
            Err(Error::ValidationError(_))
        ));
    }
    assert!(matches!(
        service
            .require(&actor(), &contract::CREATE, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::CONTRACT_IMPORT,
                AccessTarget::record("task").unwrap(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn supplier_place_candidates_and_replays_share_qualification_and_current_owner_rules() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("supplier_fulfillment_order", "order", "user")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("supplier_fulfillment_order".into(), facts.clone());
    let service = builder.build().unwrap();
    identity.state.lock().unwrap().grants = vec![("role".into(), vec![supplier_fulfillment::ORDER_SUBMIT])];
    let permission = &supplier_fulfillment::ORDER_SUBMIT;
    let mut executor = SnapshotExecutor::default();
    service
        .require(
            &actor(),
            permission,
            AccessTarget::Candidate(candidate(permission.resource(), "user")),
            &mut executor,
        )
        .await
        .unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
    service
        .require(&actor(), permission, AccessTarget::record("order").unwrap(), &mut executor)
        .await
        .unwrap();
    identity.state.lock().unwrap().grants.clear();
    for target in [
        AccessTarget::Candidate(candidate(permission.resource(), "user")),
        AccessTarget::record("order").unwrap(),
    ] {
        assert!(matches!(
            service.require(&actor(), permission, target, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn sales_sharing_uses_fixed_independent_qualification_and_the_contract_write_scope() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("contract", "contract", "user")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("contract".into(), facts.clone());
    let service = builder.build().unwrap();
    let target = AccessTarget::record("contract").unwrap();
    let permission = &internal::CONTRACT_SALES_ACCESS;
    identity.state.lock().unwrap().grants = vec![
        ("contract-writer".into(), vec![contract::TERMINATE]),
        ("assignment-manager".into(), vec![customer::ASSIGNMENT_APPLY]),
    ];
    service.require(&actor(), permission, target.clone(), &mut SnapshotExecutor::default()).await.unwrap();
    for granted in [vec![contract::TERMINATE], vec![customer::ASSIGNMENT_APPLY], vec![permission.clone()]] {
        identity.state.lock().unwrap().grants = vec![("incomplete".into(), granted)];
        assert!(matches!(
            service.require(&actor(), permission, target.clone(), &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
    identity.state.lock().unwrap().grants = vec![("writer".into(), vec![contract::TERMINATE])];
    service
        .require(&actor(), &contract::TERMINATE, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();
    identity.state.lock().unwrap().grants =
        vec![("writer".into(), vec![contract::TERMINATE, customer::ASSIGNMENT_APPLY])];
    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service.require(&actor(), permission, target.clone(), &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));
    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("user".into()));
    assert!(matches!(
        service.require(&actor(), permission, target, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(&actor(), permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
}
