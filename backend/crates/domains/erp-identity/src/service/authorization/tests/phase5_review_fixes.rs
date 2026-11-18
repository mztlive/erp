//! 第五阶段审查缺口：固定入口资格、对象范围及命令回包读取边界。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{
    customer, internal, sales_order, sales_selection, supplier_fulfillment, supplier_offering,
};
use persistence_core::Executor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::authorization::{AccessTarget, FactValue, RelatedObjects, RelationKey};
use crate::{Error, Permission};

/// 替换测试账号的启用角色资格，不为派生入口授予内部编码。
fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

#[tokio::test]
async fn offering_create_replay_uses_real_record_with_current_create_qualification() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("supplier_offering", "offering", "user")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("supplier_offering".into(), facts.clone());
    let service = builder.build().unwrap();
    let mut executor = SnapshotExecutor::default();
    let permission = internal::SUPPLIER_OFFERING_CREATED_RESULT_READ;
    grant(&identity, &[("creator", &[supplier_offering::CREATE])]);
    service
        .require(&actor(), &permission, AccessTarget::record("offering").unwrap(), &mut executor)
        .await
        .unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &supplier_offering::CREATE,
                AccessTarget::record("offering").unwrap(),
                &mut executor
            )
            .await,
        Err(Error::ValidationError(_))
    ));

    grant(&identity, &[("reader", &[supplier_offering::LIST])]);
    assert!(matches!(
        service
            .require(&actor(), &permission, AccessTarget::record("offering").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));

    grant(&identity, &[("creator", &[supplier_offering::CREATE])]);
    identity.state.lock().unwrap().denied_scope = true;
    assert!(matches!(
        service
            .require(&actor(), &permission, AccessTarget::record("offering").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));

    identity.state.lock().unwrap().active = false;
    assert!(matches!(
        service
            .require(&actor(), &permission, AccessTarget::record("offering").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn availability_requires_independent_entry_qualification_and_actual_offering_scope() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("supplier_offering", "offering", "user")]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("supplier_offering".into(), facts.clone());
    let service = builder.build().unwrap();
    let mut executor = SnapshotExecutor::default();
    let permission = internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE;
    for permissions in [vec![supplier_offering::REVISE], vec![supplier_offering::UPDATE_AVAILABILITY]] {
        grant(&identity, &[("partial", &permissions)]);
        assert!(matches!(
            service
                .require(&actor(), &permission, AccessTarget::record("offering").unwrap(), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
    }
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);

    grant(
        &identity,
        &[
            ("maintenance", &[supplier_offering::REVISE]),
            ("availability", &[supplier_offering::UPDATE_AVAILABILITY]),
        ],
    );
    service
        .require(&actor(), &permission, AccessTarget::record("offering").unwrap(), &mut executor)
        .await
        .unwrap();

    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service
            .require(&actor(), &permission, AccessTarget::record("offering").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn selection_order_basis_requires_create_before_facts_and_keeps_customer_and_proposal_rules() {
    let identity = Arc::new(Identity::default());
    let mut proposal = object("sales_selection_proposal", "proposal", "user");
    proposal.relations.insert(
        RelationKey::parse("customer").unwrap(),
        RelatedObjects {
            expected_ids: ["customer".into()].into(),
            objects: vec![object("customer", "customer", "user")],
        },
    );
    let facts = Arc::new(Facts::new(vec![proposal]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("sales_selection_proposal".into(), facts.clone());
    let service = builder.build().unwrap();
    let mut executor = SnapshotExecutor::default();
    let permission = internal::SALES_SELECTION_ORDER_BASIS;
    grant(&identity, &[("proposal", &[sales_selection::PROPOSAL_DETAIL]), ("customer", &[customer::DETAIL])]);
    assert!(matches!(service.query(&actor(), &permission, &mut executor).await, Err(Error::Forbidden(_))));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);

    grant(
        &identity,
        &[
            ("proposal", &[sales_selection::PROPOSAL_DETAIL]),
            ("customer", &[customer::DETAIL]),
            ("creation", &[sales_order::CREATE]),
        ],
    );
    service
        .require(&actor(), &permission, AccessTarget::record("proposal").unwrap(), &mut executor)
        .await
        .unwrap();

    let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
    service.validate(&access, &mut executor).await.unwrap();

    facts.objects.lock().unwrap()[0].owner_id = FactValue::Known(Some("other".into()));
    assert!(matches!(
        service
            .require(&actor(), &permission, AccessTarget::record("proposal").unwrap(), &mut executor)
            .await,
        Err(Error::Forbidden(_))
    ));
}

#[tokio::test]
async fn supplier_command_result_queries_use_original_command_without_detail_and_reject_old_stage() {
    for permission in [supplier_fulfillment::ORDER_CANCEL, supplier_fulfillment::ORDER_REFUND] {
        let identity = Arc::new(Identity::default());
        grant(&identity, &[("command", std::slice::from_ref(&permission))]);
        let mut builder = complete();
        builder.facilities = identity.clone();
        let service = builder.build().unwrap();
        let mut executor = SnapshotExecutor::default();
        let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
        service.validate(&access, &mut executor).await.unwrap();
        assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ValidationError(_))));
        let access = service.query(&actor(), &permission, &mut executor).await.unwrap();
        executor.next_read();
        assert!(matches!(service.validate(&access, &mut executor).await, Err(Error::ValidationError(_))));

        grant(&identity, &[("detail-only", &[supplier_fulfillment::ORDER_DETAIL])]);
        assert!(matches!(
            service.query(&actor(), &permission, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }
}
