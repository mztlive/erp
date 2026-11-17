use std::sync::Arc;

use erp_permissions::{
    access_control, admin, approval_instance, catalog, contract, cost, customer, organization, sales_inquiry,
    sales_order,
};
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, service};
use crate::entity::access_control::ScopeClause;
use crate::{Error, Permission};

/// 检查与业务授权共用同角色完整资格，不接受单动作或跨角色拼接。
#[tokio::test]
async fn scoped_inspection_requires_complete_same_role_qualification() {
    let permission = sales_inquiry::COMMANDS_SALES_CREATE;
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let complete = vec![permission.clone(), catalog::PRODUCT_SELLABLE_SKU_LIST];
    for grants in [
        vec![("creator".into(), vec![permission.clone()])],
        vec![
            ("creator".into(), vec![permission.clone()]),
            ("catalog".into(), vec![catalog::PRODUCT_SELLABLE_SKU_LIST]),
        ],
    ] {
        identity.state.lock().unwrap().grants = grants;
        assert!(matches!(
            service.inspection_scope(&actor(), &permission, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
    identity.state.lock().unwrap().grants = vec![("creator".into(), complete)];
    let scope = service
        .inspection_scope(&actor(), &permission, &mut SnapshotExecutor::default())
        .await
        .unwrap()
        .unwrap();
    assert!(scope.scope.has_role_scope());
}

/// 范围来源复用客户设置，但只使用合同动作本次合格的角色。
#[tokio::test]
async fn permission_source_uses_current_target_roles_and_fixed_origin() {
    let identity = Arc::new(Identity::default());
    identity.state.lock().unwrap().grants = vec![
        ("contract-reader".into(), vec![contract::DETAIL]),
        ("customer-reader".into(), vec![customer::LIST]),
    ];
    identity.role_scopes.lock().unwrap().extend([
        (("contract-reader".into(), customer::LIST), ScopeClause { self_owned: true, ..Default::default() }),
        (("customer-reader".into(), customer::LIST), ScopeClause { company: true, ..Default::default() }),
        (("contract-reader".into(), contract::DETAIL), ScopeClause { company: true, ..Default::default() }),
    ]);
    let service = service(identity, Arc::new(Facts::default()));
    let access = service
        .inspection_scope(&actor(), &contract::DETAIL, &mut SnapshotExecutor::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.scope.role_clauses, vec![ScopeClause { self_owned: true, ..Default::default() }]);
}

/// 来源与任务动作完成资格后仍不产生对象范围结论。
#[tokio::test]
async fn contextual_inspection_qualifies_all_permissions_without_object_approval() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    for (permission, companion) in [
        (cost::ENTRY_LIST, sales_order::LIST),
        (approval_instance::SUBMIT_DECISION, approval_instance::INSTANCE_LIST),
    ] {
        identity.state.lock().unwrap().grants = vec![("role".into(), vec![permission.clone()])];
        assert!(matches!(
            service.inspection_scope(&actor(), &permission, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
        identity.state.lock().unwrap().grants = vec![("role".into(), vec![permission.clone(), companion])];
        assert!(
            service
                .inspection_scope(&actor(), &permission, &mut SnapshotExecutor::default())
                .await
                .unwrap()
                .is_none()
        );
    }
    identity.state.lock().unwrap().grants =
        vec![("inspector".into(), vec![access_control::INSPECTION_INSPECT])];
    assert!(matches!(
        service
            .inspection_scope(&actor(), &access_control::INSPECTION_INSPECT, &mut SnapshotExecutor::default())
            .await,
        Err(Error::Forbidden(_))
    ));
    identity.state.lock().unwrap().grants = vec![(
        "inspector".into(),
        vec![
            access_control::INSPECTION_INSPECT,
            admin::ACCOUNT_LIST_ADMINS,
            admin::ROLE_LIST_ROLES,
            organization::LIST,
        ],
    )];
    assert!(
        service
            .inspection_scope(&actor(), &access_control::INSPECTION_INSPECT, &mut SnapshotExecutor::default())
            .await
            .unwrap()
            .is_none()
    );
}

/// 无效身份与取证失败不得退化为已通过的配置检查。
#[tokio::test]
async fn inspection_preserves_identity_denials_and_facility_errors() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    identity.state.lock().unwrap().active = false;
    assert!(matches!(
        service.inspection_scope(&actor(), &contract::DETAIL, &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));
    identity.state.lock().unwrap().fail = true;
    assert!(matches!(
        service.inspection_scope(&actor(), &contract::DETAIL, &mut SnapshotExecutor::default()).await,
        Err(Error::Internal(_))
    ));
    let unknown = Permission::parse("unknown:list").unwrap();
    identity.state.lock().unwrap().active = true;
    identity.state.lock().unwrap().fail = false;
    assert!(matches!(
        service.inspection_scope(&actor(), &unknown, &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));
}
