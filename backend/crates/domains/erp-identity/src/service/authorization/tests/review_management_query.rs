//! 管理列表经公共查询接口执行真实固定分区，不以逐对象放行代替列表过滤。
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_permissions::{approval_admin, approval_instance, internal};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::repository::authorization::testing::management_members;
use crate::{AuthorizationService, Error};

fn setup() -> (AuthorizationService, Arc<Identity>, Arc<Facts>) {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants =
        vec![("manager".into(), vec![approval_instance::INSTANCE_LIST, approval_admin::SALES_ORDER_RUNTIME])];
    let root = ObjectFacts {
        subject_type: FactValue::Known("sales_order".into()),
        relations: [(
            RelationKey::parse("subject_source").unwrap(),
            RelatedObjects {
                expected_ids: ["sale".into()].into(),
                objects: vec![object("sales_order", "sale", "user")],
            },
        )]
        .into(),
        ..object("approval_instance", "sales", "user")
    };
    let facts = Arc::new(Facts::new(vec![root]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("approval_instance".into(), facts.clone());
    (builder.build().unwrap(), identity, facts)
}

#[tokio::test]
async fn management_query_keeps_type_qualification_real_source_and_object_membership_together() {
    let (service, identity, facts) = setup();
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &internal::APPROVAL_MANAGEMENT_READ, &mut executor).await.unwrap();
    // 来源菜单权限不是管理来源的必要资格；缺失、范围外、非唯一来源与其他类型都被过滤。
    assert_eq!(management_members(&access), ["sales"]);
    service.validate(&access, &mut executor).await.unwrap();
    service
        .require(
            &actor(),
            &internal::APPROVAL_MANAGEMENT_READ,
            AccessTarget::record("sales").unwrap(),
            &mut executor,
        )
        .await
        .unwrap();
    identity.state.lock().unwrap().denied_scope = true;
    let access = service.query(&actor(), &internal::APPROVAL_MANAGEMENT_READ, &mut executor).await.unwrap();
    assert!(management_members(&access).is_empty());
    service.validate(&access, &mut executor).await.unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::APPROVAL_MANAGEMENT_READ,
                AccessTarget::record("sales").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    identity.state.lock().unwrap().denied_scope = false;
    facts.objects.lock().unwrap()[0].relations.values_mut().next().unwrap().objects.clear();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::APPROVAL_MANAGEMENT_READ,
                AccessTarget::record("sales").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn management_query_does_not_splice_type_admin_from_another_role_or_other_type() {
    let (service, identity, _) = setup();
    identity.state.lock().unwrap().grants = vec![
        ("entry".into(), vec![approval_instance::INSTANCE_LIST]),
        ("admin".into(), vec![approval_admin::SALES_ORDER_RUNTIME]),
    ];
    let mut executor = SnapshotExecutor::default();
    let access = service.query(&actor(), &internal::APPROVAL_MANAGEMENT_READ, &mut executor).await.unwrap();
    assert!(management_members(&access).is_empty());
    service.validate(&access, &mut executor).await.unwrap();
    identity.state.lock().unwrap().grants = vec![(
        "purchase-manager".into(),
        vec![approval_instance::INSTANCE_LIST, approval_admin::PURCHASE_ORDER_RUNTIME],
    )];
    let access = service.query(&actor(), &internal::APPROVAL_MANAGEMENT_READ, &mut executor).await.unwrap();
    assert_eq!(management_members(&access), ["purchase"]);
    service.validate(&access, &mut executor).await.unwrap();
}

#[tokio::test]
async fn management_query_retains_scope_expiry_and_rejects_revocation_before_delivery() {
    let (service, identity, _) = setup();
    let deadline = Instant::from_unix_secs(4_102_444_800);
    identity.scope_deadlines.lock().unwrap().insert(internal::APPROVAL_MANAGEMENT_READ, deadline);
    let mut executor = SnapshotExecutor::default();
    let mut access =
        service.query(&actor(), &internal::APPROVAL_MANAGEMENT_READ, &mut executor).await.unwrap();
    assert_eq!(access.proof.valid_until, Some(deadline));
    assert_eq!(management_members(&access), ["sales"]);
    access.proof.valid_until = Some(Instant::from_unix_secs(1));
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
    let mut executor = ReadExecutor::default();
    let access = service.query(&actor(), &internal::APPROVAL_MANAGEMENT_READ, &mut executor).await.unwrap();
    identity.state.lock().unwrap().grants = vec![("entry".into(), vec![approval_instance::INSTANCE_LIST])];
    assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ConflictError(_))));
}
