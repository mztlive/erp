//! 第五阶段缺陷经真实公共入口验证资格、目标、范围与到期拒绝。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, cost, customer, fulfillment, internal, sales_order, supplier, supplier_fulfillment,
};
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, candidate, complete, inquiry_members, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

fn bound(identity: Arc<Identity>, resource: &str, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert(resource.into(), facts);
    builder.build().unwrap()
}

fn sales_candidate(org: &str) -> ObjectFacts {
    ObjectFacts { org_unit_id: FactValue::Known(Some(org.into())), ..candidate("sales_order", "user") }
}

#[tokio::test]
async fn create_and_submit_uses_only_jointly_qualified_roles_for_creation_scope_and_replay() {
    let identity = Arc::new(Identity::default());
    grant(
        &identity,
        &[("creator", &[sales_order::CREATE]), ("joint", &[sales_order::CREATE, sales_order::SUBMIT])],
    );
    identity.role_scopes.lock().unwrap().extend([
        (("creator".into(), sales_order::CREATE), ScopeClause { company: true, ..Default::default() }),
        (
            ("joint".into(), sales_order::CREATE),
            ScopeClause { org_unit_ids: ["org-b".into()].into(), ..Default::default() },
        ),
        (("joint".into(), sales_order::SUBMIT), ScopeClause { company: true, ..Default::default() }),
    ]);
    let persisted = ObjectFacts { id: "order".into(), version: "1".into(), ..sales_candidate("org-a") };
    let facts = Arc::new(Facts::new(vec![persisted]));
    let service = bound(identity, "sales_order", facts.clone());
    let mut executor = SnapshotExecutor::default();
    // 两项普通检查各自允许，不能用这种组合代替创建入口的完整同角色范围。
    for permission in [sales_order::CREATE, sales_order::SUBMIT] {
        service
            .require(&actor(), &permission, AccessTarget::Candidate(sales_candidate("org-a")), &mut executor)
            .await
            .unwrap();
    }
    for org in ["org-a", "org-b"] {
        facts.objects.lock().unwrap()[0].org_unit_id = FactValue::Known(Some(org.into()));
        for target in [AccessTarget::Candidate(sales_candidate(org)), AccessTarget::record("order").unwrap()]
        {
            let result =
                service.require(&actor(), &internal::SALES_CREATE_AND_SUBMIT, target, &mut executor).await;
            if org == "org-b" {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
        }
    }
}

#[tokio::test]
async fn extra_cost_requires_independent_cost_qualification_and_preserves_detail_data_rules() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("sales", &[sales_order::DETAIL])]);
    let facts = Arc::new(Facts::new(vec![object("sales_order", "order", "user")]));
    let service = bound(identity.clone(), "sales_order", facts.clone());
    let mut executor = SnapshotExecutor::default();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::SALES_ORDER_EXTRA_COST_READ,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        service.query(&actor(), &internal::SALES_ORDER_EXTRA_COST_READ, &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    grant(&identity, &[("sales", &[sales_order::DETAIL]), ("cost", &[cost::ENTRY_LIST])]);
    let access =
        service.query(&actor(), &internal::SALES_ORDER_EXTRA_COST_READ, &mut executor).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one"]);
    service.validate(&access, &mut executor).await.unwrap();
    identity.scopes.lock().unwrap().insert(
        (internal::SALES_ORDER_EXTRA_COST_READ, sales_order::DETAIL),
        ResolvedScope {
            role_clauses: vec![ScopeClause { org_unit_ids: ["allowed".into()].into(), ..Default::default() }],
            user_limit: None,
        },
    );
    for (org, record_grant, allowed) in
        [("allowed", false, true), ("outside", false, false), ("outside", true, true)]
    {
        facts.objects.lock().unwrap()[0].org_unit_id = FactValue::Known(Some(org.into()));
        if record_grant {
            identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "order".into()));
        }
        let result = service
            .require(
                &actor(),
                &internal::SALES_ORDER_EXTRA_COST_READ,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await;
        if allowed {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }
    grant(&identity, &[("sales", &[sales_order::DETAIL])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::SALES_ORDER_EXTRA_COST_READ,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn fulfillment_creation_requires_each_current_functional_grant_and_active_identity() {
    let identity = Arc::new(Identity::default());
    let service = complete_with_identity(identity.clone());
    let mut executor = SnapshotExecutor::default();
    for permission in [
        fulfillment::DELIVERY_DELIVERY_CREATE,
        fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE,
        fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE,
        fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE,
    ] {
        grant(&identity, &[("creator", &[])]);
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        grant(&identity, &[("creator", std::slice::from_ref(&permission))]);
        service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await.unwrap();
        identity.state.lock().unwrap().active = false;
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        identity.state.lock().unwrap().active = true;
    }
}

fn complete_with_identity(identity: Arc<Identity>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.build().unwrap()
}

#[tokio::test]
async fn supplier_import_registration_requires_current_import_qualification() {
    let identity = Arc::new(Identity::default());
    let service = complete_with_identity(identity.clone());
    let mut executor = SnapshotExecutor::default();
    for (active, permissions, allowed) in [
        (true, vec![], false),
        (true, vec![supplier::IMPORT_SUPPLIER_IMPORT], true),
        (false, vec![supplier::IMPORT_SUPPLIER_IMPORT], false),
    ] {
        grant(&identity, &[("importer", &permissions)]);
        identity.state.lock().unwrap().active = active;
        let result = service
            .require(&actor(), &internal::SUPPLIER_IMPORT_SUBMIT, AccessTarget::Operation, &mut executor)
            .await;
        if allowed {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }
}

#[tokio::test]
async fn supplier_completion_replay_requires_one_role_with_all_three_current_permissions() {
    let identity = Arc::new(Identity::default());
    let service = complete_with_identity(identity.clone());
    let mut executor = SnapshotExecutor::default();
    let required = [
        supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
        supplier_fulfillment::ORDER_INVESTIGATION,
        supplier_fulfillment::ORDER_TASK_COMPLETION,
    ];
    grant(
        &identity,
        &[("setup", &required[..1]), ("investigate", &required[1..2]), ("complete", &required[2..])],
    );
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
                AccessTarget::Operation,
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    grant(&identity, &[("operator", &required)]);
    service
        .require(
            &actor(),
            &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
            AccessTarget::Operation,
            &mut executor,
        )
        .await
        .unwrap();
    for (active, permissions) in [(true, &required[..2]), (false, required.as_slice())] {
        grant(&identity, &[("operator", permissions)]);
        identity.state.lock().unwrap().active = active;
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
                    AccessTarget::Operation,
                    &mut executor,
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn customer_creation_accepts_candidate_and_rejects_record_and_expired_owner_change() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("customer", &[customer::CREATE, customer::UPDATE])]);
    let facts = Arc::new(Facts::new(vec![object("customer", "customer", "user")]));
    let service = bound(identity, "customer", facts);
    let mut executor = SnapshotExecutor::default();
    service
        .require(
            &actor(),
            &customer::CREATE,
            AccessTarget::Candidate(candidate("customer", "user")),
            &mut executor,
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .require(&actor(), &customer::CREATE, AccessTarget::record("customer").unwrap(), &mut executor)
            .await,
        Err(Error::ValidationError(_))
    ));
    for (expiry, expired) in [(None, false), (Some(Instant::from_unix_secs(1)), true)] {
        let destination = ObjectFacts { valid_until: expiry, ..candidate("customer", "user") };
        let result = service
            .require(
                &actor(),
                &customer::UPDATE,
                AccessTarget::OwnershipChange { id: "customer".into(), candidate: destination },
                &mut executor,
            )
            .await;
        if expired {
            assert!(matches!(result, Err(Error::ConflictError(_))));
        } else {
            result.unwrap();
        }
    }
}
