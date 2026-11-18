//! 交接目标合同和履约列表交付的公共 API 回归测试。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_core::common::time::Instant;
use erp_permissions::{Permission, sales_inquiry, supplier_fulfillment, supplier_settlement};
use persistence_core::Executor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, candidate, complete, inquiry_members, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts};
use crate::{AuthorizationService, Error};

fn bound(identity: Arc<Identity>, resource: &str, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert(resource.into(), facts);
    builder.build().unwrap()
}

fn destination(resource: &str, owner: &str, org: &str) -> ObjectFacts {
    ObjectFacts {
        id: "record".into(),
        org_unit_id: FactValue::Known(Some(org.into())),
        ..candidate(resource, owner)
    }
}

fn persisted(resource: &str, owner: &str, org: &str) -> ObjectFacts {
    ObjectFacts { org_unit_id: FactValue::Known(Some(org.into())), ..object(resource, "record", owner) }
}

fn owner_change(candidate: ObjectFacts) -> AccessTarget {
    AccessTarget::OwnershipChange { id: "record".into(), candidate }
}

fn company_scope() -> ResolvedScope {
    ResolvedScope {
        role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
        user_limit: None,
    }
}

#[tokio::test]
async fn settlement_handover_preserves_company_access_and_requires_complete_original_and_destination() {
    let permission = supplier_settlement::STATEMENT_HANDOVER;
    let resource = permission.resource();
    let identity = Arc::new(Identity::default());
    identity.scopes.lock().unwrap().insert((permission.clone(), permission.clone()), company_scope());
    let facts = Arc::new(Facts::new(vec![persisted(resource, "original-owner", "original-org")]));
    let service = bound(identity, resource, facts.clone());
    let mut executor = SnapshotExecutor::default();
    let target = destination(resource, "new-owner", "new-org");
    service.require(&actor(), &permission, owner_change(target.clone()), &mut executor).await.unwrap();
    assert_eq!(facts.calls.load(Ordering::SeqCst), 1);

    let mut missing_owner = target.clone();
    missing_owner.owner_id = FactValue::Missing;
    let mut missing_org = target.clone();
    missing_org.org_unit_id = FactValue::Missing;
    let mut wrong_resource = target.clone();
    wrong_resource.resource = "supplier".into();
    let mut forged_persisted = target.clone();
    forged_persisted.version = "persisted-version".into();
    for invalid in [missing_owner, missing_org, wrong_resource, forged_persisted] {
        assert!(matches!(
            service.require(&actor(), &permission, owner_change(invalid), &mut executor).await,
            Err(Error::ValidationError(_))
        ));
    }

    facts.objects.lock().unwrap()[0].org_unit_id = FactValue::Missing;
    assert!(matches!(
        service.require(&actor(), &permission, owner_change(target.clone()), &mut executor).await,
        Err(Error::ValidationError(_))
    ));

    facts.objects.lock().unwrap().clear();
    assert!(matches!(
        service.require(&actor(), &permission, owner_change(target), &mut executor).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn configurable_owner_changes_apply_personal_limit_to_both_original_and_destination() {
    let permission = sales_inquiry::COMMANDS_SALES_SAVE;
    let resource = permission.resource();
    for (limit, legal, outside) in [
        (
            ScopeClause { self_owned: true, ..Default::default() },
            destination(resource, "user", "new-org"),
            destination(resource, "other-owner", "new-org"),
        ),
        (
            ScopeClause { org_unit_ids: ["allowed".into()].into(), ..Default::default() },
            destination(resource, "new-owner", "allowed"),
            destination(resource, "new-owner", "outside"),
        ),
    ] {
        let identity = Arc::new(Identity::default());
        identity.scopes.lock().unwrap().insert(
            (permission.clone(), permission.clone()),
            ResolvedScope { user_limit: Some(limit), ..company_scope() },
        );
        let facts = Arc::new(Facts::new(vec![persisted(resource, "user", "allowed")]));
        let service = bound(identity, resource, facts.clone());
        let mut executor = SnapshotExecutor::default();

        service.require(&actor(), &permission, owner_change(legal.clone()), &mut executor).await.unwrap();
        assert!(matches!(
            service.require(&actor(), &permission, owner_change(outside), &mut executor).await,
            Err(Error::Forbidden(_))
        ));

        facts.objects.lock().unwrap()[0] = persisted(resource, "outside-owner", "outside");
        assert!(matches!(
            service.require(&actor(), &permission, owner_change(legal), &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn fulfillment_list_delivery_rejects_expiry_after_the_query_and_ends_successful_read_stages() {
    let permission: Permission = supplier_fulfillment::ORDER_LIST;
    let identity = Arc::new(Identity::default());
    identity.scopes.lock().unwrap().insert((permission.clone(), permission.clone()), company_scope());
    let deadline = Instant::from_unix_secs(4_102_444_800);
    identity.scope_deadlines.lock().unwrap().insert(permission.clone(), deadline);
    let service = bound(identity.clone(), permission.resource(), Arc::new(Facts::default()));
    let mut executor = SnapshotExecutor::default();

    let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one", "other"]);
    assert_eq!(access.proof.valid_until, Some(deadline));
    service.validate(&access, &mut executor).await.unwrap();
    assert!(matches!(access.ensure_current(&mut executor), Err(Error::ValidationError(_))));

    let mut access = service.query(&actor(), &permission, &mut executor).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one", "other"]);
    access.ensure_current(&mut executor).unwrap();
    // 推进已冻结的截止时间，模拟补齐展示期间到期，避免依赖真实等待。
    access.proof.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(
        service.validate(&access, &mut executor).await,
        Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED")
    ));
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), 2);
    assert!(executor.boundary().unwrap().is_snapshot());
}
