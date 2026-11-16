//! 管理操作通过真实公共入口执行完整固定资格和目标合同。
use std::sync::Arc;

use async_trait::async_trait;
use erp_permissions::{
    access_control, admin, approval_admin, approval_process, catalog, internal, organization, sales_inquiry,
    supplier_offering,
};
use mongodb::Client;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, ReadExecutor};

use super::fixtures::{Facts, Identity, actor, service};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts};
use crate::entity::resource_access::{GrantTarget, SourceUpdate};
use crate::ports::ResourceGrantFacts;
use crate::service::resource_access::ResourceGrantService;
use crate::{Error, Permission, Result};

fn grants(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

fn candidate(subject: &str) -> AccessTarget {
    AccessTarget::Candidate(ObjectFacts {
        resource: "approval_process".into(),
        subject_type: FactValue::Known(subject.into()),
        ..Default::default()
    })
}

struct NoGrantFacts;

#[async_trait]
impl ResourceGrantFacts for NoGrantFacts {
    async fn validate_targets(&self, _: &[GrantTarget], _: &mut dyn Executor) -> Result<()> {
        panic!("管理员资格检查不得读取授权目标")
    }

    async fn validate_source(&self, _: &SourceUpdate, _: &mut dyn Executor) -> Result<()> {
        panic!("管理员资格检查不得读取自动来源")
    }
}

#[tokio::test]
async fn record_grant_management_uses_the_complete_shared_scope_qualification() {
    let identity = Arc::new(Identity::default());
    let authorization = Arc::new(service(identity.clone(), Arc::new(Facts::default())));
    let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("resource_manager_unit");
    let manager = ResourceGrantService::new(db, authorization, Arc::new(NoGrantFacts));
    let required = [
        access_control::RESOURCE_GRANT_MANAGE,
        organization::CHANGE,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
    ];
    let source = (access_control::RESOURCE_GRANT_MANAGE, organization::CHANGE);
    identity.scopes.lock().unwrap().insert(
        source.clone(),
        ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        },
    );
    grants(&identity, &[("manager", &required)]);
    manager.authorize_manager(&actor(), &mut SnapshotExecutor::default()).await.unwrap();
    for roles in [
        vec![("legacy-manager", &required[..2])],
        vec![("scope", &required[..2]), ("directory", &required[2..])],
    ] {
        grants(&identity, &roles);
        assert!(matches!(
            manager.authorize_manager(&actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
    grants(&identity, &[("manager", &required)]);
    identity.scopes.lock().unwrap().get_mut(&source).unwrap().user_limit =
        Some(ScopeClause { self_owned: true, ..Default::default() });
    assert!(matches!(
        manager.authorize_manager(&actor(), &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        manager.authorize_manager(&actor(), &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn person_scope_entry_checks_same_role_company_boundary_in_unified_service() {
    for (entry, source) in [
        (access_control::PERSON_SCOPE_LIST, organization::LIST),
        (access_control::PERSON_SCOPE_SAVE, organization::CHANGE),
    ] {
        let identity = Arc::new(Identity::default());
        let service = service(identity.clone(), Arc::new(Facts::default()));
        let required = [admin::ACCOUNT_LIST_ADMINS, admin::ROLE_LIST_ROLES, entry.clone(), source.clone()];
        identity.scopes.lock().unwrap().insert(
            (entry.clone(), source.clone()),
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
        );
        grants(&identity, &[("manager", &required)]);
        service
            .require(&actor(), &entry, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await
            .unwrap();
        grants(&identity, &[("entry", &required[..3]), ("organization", &required[3..])]);
        assert!(matches!(
            service
                .require(&actor(), &entry, AccessTarget::Operation, &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        grants(&identity, &[("manager", &required)]);
        identity.scopes.lock().unwrap().get_mut(&(entry.clone(), source)).unwrap().user_limit =
            Some(ScopeClause { self_owned: true, ..Default::default() });
        assert!(matches!(
            service
                .require(&actor(), &entry, AccessTarget::Operation, &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        assert!(matches!(
            service.require(&actor(), &entry, AccessTarget::Operation, &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn definition_mutations_require_entry_and_actual_type_before_allowing_the_candidate() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    for entry in [
        approval_process::CREATE_DEFINITION_DRAFT,
        approval_process::REPLACE_DEFINITION_NODES,
        approval_process::PUBLISH_DEFINITION,
        approval_process::RETIRE_DEFINITION,
    ] {
        grants(&identity, &[("type", &[approval_admin::SALES_ORDER_DEFINITION])]);
        assert!(matches!(
            service
                .require(&actor(), &entry, candidate("sales_order"), &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        grants(&identity, &[("entry", std::slice::from_ref(&entry))]);
        assert!(matches!(
            service
                .require(&actor(), &entry, candidate("sales_order"), &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        grants(
            &identity,
            &[("entry", std::slice::from_ref(&entry)), ("type", &[approval_admin::SALES_ORDER_DEFINITION])],
        );
        service
            .require(&actor(), &entry, candidate("sales_order"), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert!(matches!(
            service
                .require(&actor(), &entry, candidate("purchase_order"), &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        assert!(matches!(
            service
                .require(&actor(), &entry, AccessTarget::Operation, &mut SnapshotExecutor::default())
                .await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            service.require(&actor(), &entry, candidate("unknown"), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn runtime_admin_can_read_a_definition_but_cannot_edit_or_choose_definition_assignees() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    grants(
        &identity,
        &[
            ("entry", &[approval_process::DEFINITION_DETAIL]),
            ("type", &[approval_admin::SALES_ORDER_RUNTIME]),
        ],
    );
    service
        .require(
            &actor(),
            &approval_process::DEFINITION_DETAIL,
            candidate("sales_order"),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::APPROVAL_DEFINITION_ASSIGNEES,
                candidate("sales_order"),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    grants(
        &identity,
        &[
            ("entry", &[approval_process::DEFINITION_DETAIL]),
            ("type", &[approval_admin::SALES_ORDER_DEFINITION]),
        ],
    );
    service
        .require(
            &actor(),
            &internal::APPROVAL_DEFINITION_ASSIGNEES,
            candidate("sales_order"),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn transfer_prechecks_use_fixed_independent_qualifications_without_internal_grants() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    grants(
        &identity,
        &[
            ("buyer", &[sales_inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES]),
            ("catalog", &[catalog::PRODUCT_SELLABLE_SKU_LIST]),
            ("supplier", &[supplier_offering::LIST]),
        ],
    );
    service
        .require(
            &actor(),
            &internal::DATA_TRANSFER_SUPPLIER_QUOTES,
            AccessTarget::Operation,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
    identity.state.lock().unwrap().grants.pop();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::DATA_TRANSFER_SUPPLIER_QUOTES,
                AccessTarget::Operation,
                &mut SnapshotExecutor::default(),
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::DATA_TRANSFER_SUPPLIER_QUOTES,
                candidate("sales_order"),
                &mut SnapshotExecutor::default(),
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}
