//! 管理入口及命令回包通过真实统一 API 验证，测试不连接数据库。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_permissions::{access_control, admin, internal, purchase_order, sales_order, sales_review};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::*;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::service::iam::management::ManagementAccess;
use crate::{AccessTarget, AuthorizationService, Error, Permission};

fn grant(identity: &Identity, permissions: Vec<(String, Vec<Permission>)>) {
    identity.state.lock().unwrap().grants = permissions;
}

#[tokio::test]
async fn management_entries_require_their_own_grant_and_recheck_after_revocation() {
    let identity = Arc::new(Identity::default());
    let service = Arc::new(service(identity.clone(), Arc::new(Facts::default())));
    for permission in [
        admin::ACCOUNT_CREATE_ADMIN,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ACCOUNT_UPDATE_ADMIN,
        admin::ACCOUNT_DELETE_ADMIN,
        admin::ACCOUNT_UPDATE_ADMIN_ROLE,
        admin::ROLE_CREATE_ROLE,
        admin::ROLE_LIST_ROLES,
        admin::ROLE_LIST_ASSIGNABLE_ROLES,
        admin::ROLE_UPDATE_ROLE,
        admin::ROLE_DELETE_ROLE,
        access_control::PERMISSION_LIST,
        access_control::PERMISSION_CREATE,
        access_control::PERMISSION_UPDATE,
        access_control::PERMISSION_DELETE,
        access_control::USER_ROLE_LIST,
        access_control::USER_ROLE_CREATE,
        access_control::USER_ROLE_REVOKE,
        access_control::AUDIT_EVENT_LIST,
    ] {
        let access = ManagementAccess::new(service.clone(), actor(), &permission);
        grant(&identity, vec![("ordinary".into(), vec![sales_order::LIST])]);
        assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
        grant(&identity, vec![("manager".into(), vec![permission.clone()])]);
        access.require(&mut SnapshotExecutor::default()).await.unwrap();
        grant(&identity, vec![("manager".into(), vec![])]);
        assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
    }
}

#[tokio::test]
async fn management_never_uses_a_previously_valid_identity_or_masks_facility_errors() {
    let identity = Arc::new(Identity::default());
    let service = Arc::new(service(identity.clone(), Arc::new(Facts::default())));
    let access = ManagementAccess::new(service, actor(), &admin::ROLE_CREATE_ROLE);
    access.require(&mut SnapshotExecutor::default()).await.unwrap();
    identity.state.lock().unwrap().active = false;
    assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
    identity.state.lock().unwrap().active = true;
    identity.state.lock().unwrap().fail = true;
    assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Internal(_))));
    let calls = identity.permissions_calls.load(Ordering::SeqCst);
    assert!(matches!(access.require(&mut ReadExecutor::default()).await, Err(Error::ValidationError(_))));
    assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), calls);
}

fn purchase_service(identity: Arc<Identity>, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert("purchase_order".into(), facts);
    builder.build().unwrap()
}

#[tokio::test]
async fn purchase_change_replies_require_original_entry_and_source_without_detail_menu_grants() {
    for (permission, entry, source) in [
        (
            internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ,
            purchase_order::PURCHASE_CHANGE_SUBMIT,
            purchase_order::SUBMIT,
        ),
        (
            internal::PURCHASE_CHANGE_VOIDED_RESULT_READ,
            purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID,
            purchase_order::SAVE_DRAFT,
        ),
    ] {
        let identity = Arc::new(Identity::default());
        identity.scopes.lock().unwrap().insert(
            (permission.clone(), permission.clone()),
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
        );
        let facts = Arc::new(Facts::new(vec![object("purchase_order", "purchase", "other")]));
        let service = purchase_service(identity.clone(), facts.clone());
        let target = || AccessTarget::record("purchase").unwrap();
        for granted in [vec![entry.clone()], vec![source.clone()]] {
            grant(&identity, vec![("incomplete".into(), granted)]);
            assert!(matches!(
                service.require(&actor(), &permission, target(), &mut SnapshotExecutor::default()).await,
                Err(Error::Forbidden(_))
            ));
        }
        assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
        grant(&identity, vec![("entry".into(), vec![entry]), ("source".into(), vec![source])]);
        service.require(&actor(), &permission, target(), &mut SnapshotExecutor::default()).await.unwrap();
        let mut executor = SnapshotExecutor::default();
        service.require(&actor(), &permission, target(), &mut executor).await.unwrap();
        executor.next_read();
        identity.state.lock().unwrap().active = false;
        assert!(matches!(
            service.require(&actor(), &permission, target(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }
}

#[tokio::test]
async fn source_order_reading_does_not_supply_sales_or_purchase_change_entry_grants() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    grant(
        &identity,
        vec![(
            "orders".into(),
            vec![sales_order::LIST, sales_order::DETAIL, purchase_order::LIST, purchase_order::DETAIL],
        )],
    );
    for permission in [
        sales_review::SALES_CHANGE_ORDER_LIST,
        sales_review::SALES_CHANGE_ORDER_DETAIL,
        purchase_order::PURCHASE_CHANGE_LIST,
        purchase_order::PURCHASE_CHANGE_DETAIL,
    ] {
        assert!(matches!(
            service
                .require(&actor(), &permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        identity.state.lock().unwrap().grants.push(("entry".into(), vec![permission.clone()]));
        service
            .require(&actor(), &permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await
            .unwrap();
    }
}
