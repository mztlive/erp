//! 管理操作通过真实公共入口执行完整固定资格和目标合同。
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::{
    access_control, admin, approval_admin, approval_process, catalog, internal, organization, sales_inquiry,
    supplier_offering,
};
use mongodb::Client;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, ReadExecutor};

use super::super::facilities::Facilities;
use super::fixtures::{Facts, Identity, actor, complete, service};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessContext, AccessTarget, FactValue, ObjectFacts, ResourceTargets};
use crate::entity::resource_access::event::GrantEvent;
use crate::entity::resource_access::{GrantTarget, ResourceKind, SourceUpdate};
use crate::ports::ResourceGrantFacts;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::service::resource_access::ResourceGrantService;
use crate::{Error, Permission, QualifiedPermissions, Result};

// 替换测试账号的角色授权，逐项核对管理入口的组合要求。
fn grants(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

// 构造指定审批主体类型的定义候选，不绑定持久化记录。
fn candidate(subject: &str) -> AccessTarget {
    AccessTarget::Candidate(ObjectFacts {
        resource: "approval_process".into(),
        subject_type: FactValue::Known(subject.into()),
        ..Default::default()
    })
}

// 一旦资格检查误读授权目标或来源就失败的事实替身。
struct NoGrantFacts;

// 用必定失败的实现守住管理资格检查不读业务事实的边界。
#[async_trait]
impl ResourceGrantFacts for NoGrantFacts {
    // 若资格检查尝试校验记录授权目标，立即让测试失败。
    async fn validate_targets(&self, _: &[GrantTarget], _: &mut dyn Executor) -> Result<()> {
        panic!("管理员资格检查不得读取授权目标")
    }

    // 若资格检查尝试计算自动授权来源，立即让测试失败。
    async fn compute_source(&self, _: &GrantEvent, _: &mut dyn Executor) -> Result<SourceUpdate> {
        panic!("管理员资格检查不得读取自动来源")
    }
}

// 记录授权管理沿用统一范围管理的完整资格，不接受缺项或无效目标。
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

// 人员范围配置要求管理资格同角色且覆盖公司范围。
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

// 修改审批定义既要入口权限也要实际主体类型的管理资格，只接受正确候选。
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

// 运行管理员可以读取定义，但不能编辑定义或查询定义经办人候选。
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

// 读取审批定义候选前先检查快照，不用非事务预检读取身份。
#[tokio::test]
async fn definition_read_candidates_require_snapshot_before_reading_permissions() {
    let identity = Arc::new(Identity::default());
    grants(
        &identity,
        &[("reader", &[approval_process::DEFINITION_DETAIL, approval_admin::SALES_ORDER_DEFINITION])],
    );
    let service = service(identity.clone(), Arc::new(Facts::default()));
    for permission in [approval_process::DEFINITION_DETAIL, internal::APPROVAL_DEFINITION_ASSIGNEES] {
        let mut read = ReadExecutor::default();
        let mut unbound = NoTransaction;
        for executor in [&mut read as &mut dyn Executor, &mut unbound] {
            let calls = identity.permissions_calls.load(Ordering::SeqCst);
            assert!(matches!(
                service.require(&actor(), &permission, candidate("sales_order"), executor).await,
                Err(Error::ValidationError(message)) if message == "审批定义授权必须使用快照事务"
            ));
            assert_eq!(identity.permissions_calls.load(Ordering::SeqCst), calls);
        }
        service
            .require(&actor(), &permission, candidate("sales_order"), &mut SnapshotExecutor::default())
            .await
            .unwrap();
    }
}

// 审批定义候选到期时报数据范围已变，不能继续使用授权结果。
#[tokio::test]
async fn expired_definition_candidates_report_data_scope_changed() {
    let identity = Arc::new(Identity::default());
    grants(
        &identity,
        &[("reader", &[approval_process::DEFINITION_DETAIL, approval_admin::SALES_ORDER_DEFINITION])],
    );
    let service = service(identity, Arc::new(Facts::default()));
    for permission in [approval_process::DEFINITION_DETAIL, internal::APPROVAL_DEFINITION_ASSIGNEES] {
        let AccessTarget::Candidate(mut object) = candidate("sales_order") else { unreachable!() };
        object.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &permission,
                    AccessTarget::Candidate(object),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED")
        ));
    }
}

// 在权限读取中推进读取阶段，模拟审批定义授权期间阶段失效。
struct AdvancingDefinitionIdentity;

// 只实现正常资格返回；调用对象范围或记录授权接口视为错误。
#[async_trait]
impl Facilities for AdvancingDefinitionIdentity {
    // 推进执行器读取阶段后返回完整权限，模拟资格读取跨阶段。
    async fn permissions(
        &self,
        _: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        executor.next_read();
        Ok(Some(QualifiedPermissions::test_grants(&[("role", permissions)])))
    }

    // 若定义候选授权读取对象范围，立即让测试失败。
    async fn scope(
        &self,
        _: &AccessContext,
        _: &Permission,
        _: &[Permission],
        _: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        panic!("审批定义候选不得读取对象范围")
    }

    // 若定义候选授权读取记录授权版本，立即让测试失败。
    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        panic!("审批定义候选不得读取记录授权版本")
    }

    // 若定义候选授权读取记录授权，立即让测试失败。
    async fn grants(
        &self,
        _: &AccessContext,
        _: ResourceKind,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        panic!("审批定义候选不得读取记录授权")
    }
}

// 读取定义资格过程中若跨读取阶段，即使权限齐全也报数据范围已变。
#[tokio::test]
async fn definition_permissions_crossing_read_stages_report_data_scope_changed() {
    let mut builder = complete();
    builder.facilities = Arc::new(AdvancingDefinitionIdentity);
    let service = builder.build().unwrap();
    for permission in [approval_process::DEFINITION_DETAIL, internal::APPROVAL_DEFINITION_ASSIGNEES] {
        assert!(matches!(
            service
                .require(&actor(), &permission, candidate("sales_order"), &mut SnapshotExecutor::default())
                .await,
            Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED")
        ));
    }
}

// 数据迁移预检要求各自独立的固定业务资格，不授予内部权限码，也不接受对象候选。
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
