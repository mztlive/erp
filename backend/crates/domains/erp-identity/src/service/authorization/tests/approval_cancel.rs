//! 普通库存撤回通过真实统一入口保留提交人、管理员和独立详情范围合同。
use std::sync::Arc;

use erp_permissions::{approval_admin, approval_instance, inventory};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, complete, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::{AuthorizationService, Error, Permission, Result};

fn instance(owner: &str) -> ObjectFacts {
    let source = ObjectFacts {
        warehouse_id: FactValue::Known(Some("warehouse".into())),
        relations: [(
            RelationKey::parse("warehouse").unwrap(),
            RelatedObjects {
                expected_ids: ["warehouse".into()].into(),
                objects: vec![object("warehouse", "warehouse", "other")],
            },
        )]
        .into(),
        ..object("stock_adjustment", "adjustment", "other")
    };
    ObjectFacts {
        subject_type: FactValue::Known("stock_adjustment".into()),
        relations: [(
            RelationKey::parse("subject_source").unwrap(),
            RelatedObjects { expected_ids: ["adjustment".into()].into(), objects: vec![source] },
        )]
        .into(),
        ..object("approval_instance", "instance", owner)
    }
}

fn setup(owner: &str) -> (AuthorizationService, Arc<Identity>, Arc<Facts>) {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![instance(owner)]));
    let mut builder = complete();
    builder.facilities = identity.clone();
    builder.facts.insert("approval_instance".into(), facts.clone());
    identity.scopes.lock().unwrap().insert(
        (inventory::STOCK_ADJUSTMENT_DETAIL, inventory::STOCK_ADJUSTMENT_DETAIL),
        ResolvedScope {
            role_clauses: vec![ScopeClause {
                warehouse_ids: ["warehouse".into()].into(),
                ..Default::default()
            }],
            user_limit: None,
        },
    );
    (builder.build().unwrap(), identity, facts)
}

fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

async fn cancel(service: &AuthorizationService) -> Result<()> {
    service
        .require(
            &actor(),
            &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
            AccessTarget::record("instance").unwrap(),
            &mut SnapshotExecutor::default(),
        )
        .await
}

#[tokio::test]
async fn original_submitter_requires_same_role_cancel_and_read_without_stock_detail_menu_or_scope() {
    let (service, identity, _) = setup("user");
    let required = [inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, approval_instance::INSTANCE_LIST];
    grant(&identity, &[("submitter", &required)]);
    identity.state.lock().unwrap().denied_scope = true;
    identity.scopes.lock().unwrap().clear();
    cancel(&service).await.unwrap();
    for roles in [
        vec![("cancel", &required[..1])],
        vec![("read", &required[1..])],
        vec![("cancel", &required[..1]), ("read", &required[1..])],
    ] {
        grant(&identity, &roles);
        assert!(matches!(cancel(&service).await, Err(Error::Forbidden(_))));
    }
}

#[tokio::test]
async fn runtime_admin_preserves_same_role_management_and_independent_stock_detail() {
    let (service, identity, _) = setup("other");
    let required = [
        inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
        approval_instance::INSTANCE_LIST,
        approval_admin::STOCK_ADJUSTMENT_RUNTIME,
    ];
    grant(&identity, &[("manager", &required), ("stock-reader", &[inventory::STOCK_ADJUSTMENT_DETAIL])]);
    cancel(&service).await.unwrap();

    for missing in 0..required.len() {
        let partial = required
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != missing)
            .map(|(_, permission)| permission.clone())
            .collect::<Vec<_>>();
        grant(
            &identity,
            &[
                ("partial", &partial),
                ("split", &required[missing..missing + 1]),
                ("stock-reader", &[inventory::STOCK_ADJUSTMENT_DETAIL]),
            ],
        );
        assert!(matches!(cancel(&service).await, Err(Error::Forbidden(_))));
    }

    grant(&identity, &[("manager", &required)]);
    assert!(matches!(cancel(&service).await, Err(Error::Forbidden(_))));

    grant(&identity, &[("manager", &required), ("stock-reader", &[inventory::STOCK_ADJUSTMENT_DETAIL])]);
    identity.scopes.lock().unwrap().values_mut().next().unwrap().role_clauses[0].warehouse_ids =
        ["other-warehouse".into()].into();
    assert!(matches!(cancel(&service).await, Err(Error::Forbidden(_))));
}

#[tokio::test]
async fn cancellation_rejects_missing_sources_wrong_types_and_unversioned_facts_even_for_submitter() {
    let (service, identity, facts) = setup("user");
    grant(
        &identity,
        &[("submitter", &[inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, approval_instance::INSTANCE_LIST])],
    );
    for invalid in ["missing-source", "wrong-type", "missing-warehouse", "wrong-warehouse", "version"] {
        let mut root = instance("user");
        match invalid {
            "missing-source" => root.relations.values_mut().next().unwrap().objects.clear(),
            "wrong-type" => root.subject_type = FactValue::Known("sales_order".into()),
            "missing-warehouse" => root.relations.values_mut().next().unwrap().objects[0].relations.clear(),
            "wrong-warehouse" => {
                root.relations.values_mut().next().unwrap().objects[0].warehouse_id =
                    FactValue::Known(Some("other-warehouse".into()))
            },
            "version" => root.version.clear(),
            _ => unreachable!(),
        }
        *facts.objects.lock().unwrap() = vec![root];
        assert!(matches!(cancel(&service).await, Err(Error::ValidationError(_))), "{invalid}");
    }

    assert!(matches!(
        service
            .require(
                &actor(),
                &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
                AccessTarget::Operation,
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
                AccessTarget::record("instance").unwrap(),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}
