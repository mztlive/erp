//! 接收者、形成前执行资格和展示共用固定任务组合，测试执行真实公共入口。
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, approval_instance, fulfillment, internal, purchase_order, purchase_payment_request,
    sales_inquiry, sales_order, work_item,
};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

/// 替身只提供真实事实和账号角色，授权组合仍执行生产实现。
fn service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("work_item".into(), facts);
    builder.build().unwrap()
}

/// 配置本次账号的真实直接角色资格。
fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

/// 候选只表达尚未形成任务的服务端种类与来源类型。
fn candidate(kind: &str, source: &str) -> ObjectFacts {
    ObjectFacts {
        resource: "work_item".into(),
        subject_type: FactValue::Known(kind.into()),
        subject_source_resource: FactValue::Known(source.into()),
        ..Default::default()
    }
}

/// 当前任务仍属于其他人，接收者只按真实业务来源检查。
fn task() -> ObjectFacts {
    ObjectFacts {
        id: "task".into(),
        version: "v1".into(),
        owner_id: FactValue::Known(Some("other".into())),
        org_unit_id: FactValue::Known(None),
        assignee_id: FactValue::Known(Some("other".into())),
        relations: [(
            RelationKey::parse("order_source").unwrap(),
            RelatedObjects {
                expected_ids: ["purchase".into()].into(),
                objects: vec![object("purchase_order", "purchase", "user")],
            },
        )]
        .into(),
        ..candidate("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order")
    }
}

/// 客户验收执行资格不包含销售单详情，不能由新入口追加。
fn acceptance() -> [Permission; 5] {
    [
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
        fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
        fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
        sales_order::EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD,
    ]
}

#[tokio::test]
async fn formation_checks_execution_only_and_rejects_split_roles() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let required = acceptance();
    grant(&identity, &[("executor", &required)]);
    let target = AccessTarget::Candidate(candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order"));
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            target.clone(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    for missing in 0..required.len() {
        let partial = required
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != missing)
            .map(|(_, permission)| permission.clone())
            .collect::<Vec<_>>();
        grant(&identity, &[("partial", &partial), ("other", &required[missing..missing + 1])]);
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                    target.clone(),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn formation_retains_rework_detail_requirement_without_adding_source_read() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let target = AccessTarget::Candidate(candidate("APPROVAL_REWORK", "sales_order"));
    grant(&identity, &[("source-reader", &[sales_order::DETAIL])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    grant(&identity, &[("task-reader", &[work_item::DETAIL])]);
    service
        .require(
            &actor(),
            &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
            target,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn formation_rejects_untrusted_shape_unknown_kind_expiry_and_non_snapshot() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    for invalid in [
        candidate("UNKNOWN", "sales_order"),
        candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "wrong"),
        ObjectFacts {
            id: "persisted".into(),
            ..candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order")
        },
        ObjectFacts { version: "v1".into(), ..candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order") },
    ] {
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                    AccessTarget::Candidate(invalid),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::ValidationError(_))
        ));
    }
    let valid = candidate("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order");
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(valid.clone()),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    let expired = ObjectFacts { valid_until: Some(Instant::from_unix_secs(1)), ..valid.clone() };
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(expired),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ConflictError(_))
    ));
    identity.state.lock().unwrap().active = false;
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_EXECUTION_QUALIFICATION,
                AccessTarget::Candidate(valid),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn recipient_loads_exact_task_without_requiring_current_ownership() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![task()]));
    let service = service(identity.clone(), facts.clone());
    let required =
        [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    grant(&identity, &[("executor", &required), ("source-reader", &[work_item::DETAIL])]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();
    for (id, version) in [("task", "old"), ("missing", "v1")] {
        assert!(
            service
                .require(
                    &actor(),
                    &internal::WORK_ITEM_RECIPIENT,
                    AccessTarget::Task { id: id.into(), version: version.into() },
                    &mut SnapshotExecutor::default()
                )
                .await
                .is_err()
        );
    }
    grant(
        &identity,
        &[("partial", &required[..2]), ("other", &required[2..]), ("source-reader", &[work_item::DETAIL])],
    );
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    facts.objects.lock().unwrap()[0].subject_type = FactValue::Known("UNKNOWN".into());
    assert!(matches!(
        service
            .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                AccessTarget::Candidate(task()),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn recipient_preserves_independent_source_roles_and_rejects_missing_real_source() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![task()]));
    let service = service(identity.clone(), facts.clone());
    let required =
        [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    grant(&identity, &[("executor", &required), ("source", &[work_item::DETAIL])]);
    identity.role_scopes.lock().unwrap().extend([
        (("executor".into(), purchase_order::DETAIL), ScopeClause::default()),
        (("source".into(), purchase_order::DETAIL), ScopeClause { self_owned: true, ..Default::default() }),
    ]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();
    grant(&identity, &[("executor", &required)]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    facts.objects.lock().unwrap()[0].relations.clear();
    assert!(matches!(
        service
            .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn fulfillment_recipient_preserves_task_read_roles_without_adding_order_menu_permissions() {
    let identity = Arc::new(Identity::default());
    let mut item = task();
    item.subject_type = FactValue::Known("FULFILLMENT_OPERATION".into());
    item.subject_source_resource = FactValue::Known("delivery".into());
    item.relations.values_mut().next().unwrap().objects = vec![object("sales_order", "purchase", "user")];
    let facts = Arc::new(Facts::new(vec![item]));
    let service = service(identity.clone(), facts.clone());
    let full = [
        fulfillment::DELIVERY_DELIVERY_LIST,
        fulfillment::DELIVERY_DELIVERY_DETAIL,
        fulfillment::DELIVERY_DELIVERY_UPDATE,
        fulfillment::DELIVERY_DELIVERY_POST,
    ];
    grant(&identity, &[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();
    grant(&identity, &[("executor", &full)]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    grant(&identity, &[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    {
        let mut roots = facts.objects.lock().unwrap();
        let source = &mut roots[0].relations.values_mut().next().unwrap().objects[0];
        source.owner_id = FactValue::Known(Some("other".into()));
        source.history_participants = FactValue::Known([actor().id().into()].into());
    }
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "purchase".into()));
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn recipient_purchase_source_retains_history_and_explicit_record_read_supplements() {
    let identity = Arc::new(Identity::default());
    let mut item = task();
    {
        let source = &mut item.relations.values_mut().next().unwrap().objects[0];
        source.owner_id = FactValue::Known(Some("other".into()));
        source.history_participants = FactValue::Known([actor().id().into()].into());
    }
    let facts = Arc::new(Facts::new(vec![item]));
    let service = service(identity.clone(), facts.clone());
    let full = [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    grant(&identity, &[("executor", &full), ("task-reader", &[work_item::DETAIL])]);
    let target = AccessTarget::Task { id: "task".into(), version: "v1".into() };
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target.clone(), &mut SnapshotExecutor::default())
        .await
        .unwrap();
    facts.objects.lock().unwrap()[0].relations.values_mut().next().unwrap().objects[0].history_participants =
        FactValue::Known(Default::default());
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::WORK_ITEM_RECIPIENT,
                target.clone(),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    identity.record_grants.lock().unwrap().insert((ResourceKind::PurchaseOrder, "purchase".into()));
    service
        .require(&actor(), &internal::WORK_ITEM_RECIPIENT, target, &mut SnapshotExecutor::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn query_qualification_and_recipient_use_the_same_complete_task_combinations() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::new(vec![task()])));
    let full = [purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT];
    for allowed in [true, false] {
        let partial = if allowed { full.as_slice() } else { &full[..2] };
        grant(
            &identity,
            &[
                ("reader", &[work_item::LIST, work_item::DETAIL, work_item::MANAGE]),
                ("executor", partial),
                ("other", &full[2..]),
                ("decision", &[approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION]),
            ],
        );
        for permission in [work_item::LIST, work_item::DETAIL, work_item::MANAGE] {
            let mut executor = SnapshotExecutor::default();
            let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
            assert_eq!(
                access
                    .execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut executor)
                    .unwrap(),
                allowed
            );
            assert!(access.decision_qualified(&mut executor).unwrap());
            service.validate(&access, &mut executor).await.unwrap();
            assert_eq!(
                service
                    .require(
                        &actor(),
                        &internal::WORK_ITEM_RECIPIENT,
                        AccessTarget::Task { id: "task".into(), version: "v1".into() },
                        &mut executor
                    )
                    .await
                    .is_ok(),
                allowed
            );
        }
    }
}

#[tokio::test]
async fn query_qualification_rejects_other_queries_expiry_and_cross_stage_reuse() {
    let service = complete().build().unwrap();
    let mut executor = SnapshotExecutor::default();
    let other = service.query(&actor(), &sales_inquiry::QUERY_SALES_LIST, &mut executor).await.unwrap();
    assert!(matches!(other.decision_qualified(&mut executor), Err(Error::ValidationError(_))));
    let mut access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert!(
        access.execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut executor).unwrap()
    );
    access.proof.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(access.decision_qualified(&mut executor), Err(Error::ConflictError(_))));
    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    executor.next_read();
    assert!(matches!(access.decision_qualified(&mut executor), Err(Error::ValidationError(_))));
    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    executor.restart();
    assert!(matches!(access.decision_qualified(&mut executor), Err(Error::ValidationError(_))));
}

#[tokio::test]
async fn optional_qualification_revocation_invalidates_the_original_nontransaction_query() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    grant(
        &identity,
        &[
            ("reader", &[work_item::LIST]),
            (
                "executor",
                &[purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT],
            ),
        ],
    );
    let mut executor = ReadExecutor::default();
    let access = service.query(&actor(), &work_item::LIST, &mut executor).await.unwrap();
    assert!(
        access.execution_qualified("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", &mut executor).unwrap()
    );
    grant(&identity, &[("reader", &[work_item::LIST])]);
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
}
