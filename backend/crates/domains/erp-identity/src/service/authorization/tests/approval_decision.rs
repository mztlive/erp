//! 执行真实 DECIDE 公共入口，验证精确审批任务和来源范围。
use erp_permissions::{approval_instance, purchase_order, sales_order};

use super::*;
use crate::entity::resource_access::SourceKey;

fn task(kind: &str, resource: &str) -> ObjectFacts {
    let mut task = object("approval_instance", "task", "user");
    task.subject_type = FactValue::Known(kind.into());
    task.assignee_id = FactValue::Known(Some("user".into()));
    task.relations.insert(
        RelationKey::parse("subject_source").unwrap(),
        RelatedObjects {
            expected_ids: ["order".into()].into(),
            objects: vec![object(resource, "order", "other")],
        },
    );
    task.relations.insert(
        RelationKey::parse("decision_instance").unwrap(),
        RelatedObjects {
            expected_ids: ["instance".into()].into(),
            objects: vec![ObjectFacts {
                subject_type: FactValue::Known(kind.into()),
                ..object("approval_instance", "instance", "submitter")
            }],
        },
    );
    task
}

fn setup(task: ObjectFacts) -> (AuthorizationService, Arc<Identity>, Arc<Facts>) {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants =
        vec![("approver".into(), vec![approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])];
    let facts = Arc::new(Facts::new(vec![task]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("approval_instance".into(), facts.clone());
    (builder.build().unwrap(), identity, facts)
}

fn key(resource: ResourceKind, instance: &str) -> SourceKey {
    SourceKey {
        rule: match resource {
            ResourceKind::SalesOrder => "sales_order_approval_assigned",
            ResourceKind::PurchaseOrder => "purchase_order_approval_assigned",
        }
        .into(),
        business_type: "approval_instance".into(),
        business_id: instance.into(),
        recipient: "user".into(),
    }
}

async fn decide(service: &AuthorizationService) -> Result<()> {
    service
        .require(
            &actor(),
            &approval_instance::SUBMIT_DECISION,
            AccessTarget::Task { id: "task".into(), version: "v1".into() },
            &mut SnapshotExecutor::default(),
        )
        .await
}

#[tokio::test]
async fn decision_uses_approver_role_scope_without_requiring_source_menu_permission() {
    for (kind, resource, permission) in [
        ("sales_order", "sales_order", sales_order::DETAIL),
        ("purchase_order", "purchase_order", purchase_order::DETAIL),
    ] {
        let (service, identity, _) = setup(task(kind, resource));
        identity
            .role_scopes
            .lock()
            .unwrap()
            .insert(("approver".into(), permission), ScopeClause { company: true, ..Default::default() });
        decide(&service).await.unwrap();
        identity.role_scopes.lock().unwrap().clear();
        assert!(matches!(decide(&service).await, Err(Error::Forbidden(_))));
    }
}

#[tokio::test]
async fn only_current_instance_recipient_rule_and_real_target_grant_can_supplement_scope() {
    for (kind, resource, target_kind) in [
        ("sales_order", "sales_order", ResourceKind::SalesOrder),
        ("purchase_order", "purchase_order", ResourceKind::PurchaseOrder),
    ] {
        let (service, identity, _) = setup(task(kind, resource));
        identity.record_grants.lock().unwrap().insert((target_kind, "order".into()));
        assert!(matches!(decide(&service).await, Err(Error::Forbidden(_))));
        for source in [
            key(target_kind, "different-instance"),
            SourceKey { recipient: "other".into(), ..key(target_kind, "instance") },
            SourceKey { rule: "manual_read".into(), ..key(target_kind, "instance") },
        ] {
            identity.source_grants.lock().unwrap().insert((
                source.id().unwrap(),
                target_kind,
                "order".into(),
            ));
            assert!(matches!(decide(&service).await, Err(Error::Forbidden(_))));
        }
        identity.source_grants.lock().unwrap().insert((
            key(target_kind, "instance").id().unwrap(),
            target_kind,
            "unrelated-order".into(),
        ));
        assert!(matches!(decide(&service).await, Err(Error::Forbidden(_))));
        identity.source_grants.lock().unwrap().insert((
            key(target_kind, "instance").id().unwrap(),
            target_kind,
            "order".into(),
        ));
        decide(&service).await.unwrap();
    }
}

#[tokio::test]
async fn wrong_source_missing_instance_split_roles_and_reassignment_fail_closed() {
    let (service, identity, facts) = setup(task("sales_order", "sales_order"));
    identity.source_grants.lock().unwrap().insert((
        key(ResourceKind::SalesOrder, "instance").id().unwrap(),
        ResourceKind::SalesOrder,
        "order".into(),
    ));
    decide(&service).await.unwrap();
    identity.state.lock().unwrap().grants = vec![
        ("reader".into(), vec![approval_instance::INSTANCE_LIST]),
        ("decider".into(), vec![approval_instance::SUBMIT_DECISION]),
    ];
    assert!(matches!(decide(&service).await, Err(Error::Forbidden(_))));
    identity.state.lock().unwrap().grants =
        vec![("approver".into(), vec![approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])];
    facts.objects.lock().unwrap()[0].assignee_id = FactValue::Known(Some("other".into()));
    assert!(matches!(decide(&service).await, Err(Error::Forbidden(_))));
    let mut wrong = task("sales_order", "purchase_order");
    *facts.objects.lock().unwrap() = vec![wrong.clone()];
    assert!(matches!(decide(&service).await, Err(Error::ValidationError(_))));
    wrong = task("sales_order", "sales_order");
    wrong.relations.remove(&RelationKey::parse("decision_instance").unwrap());
    *facts.objects.lock().unwrap() = vec![wrong];
    assert!(matches!(decide(&service).await, Err(Error::ValidationError(_))));
}
