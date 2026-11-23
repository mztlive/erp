#![cfg(test)]
//! 访问检查、授权文件、组织/记录授权/审批定义管理、纯功能操作与后台任务授权测试。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, access_control, admin, approval_admin, approval_instance, approval_process, bulk_job,
    catalog, contract, cost, customer, demo_master_data, fulfillment, internal, organization, party, payable,
    purchase_order, purchase_payment_request, receivable, sales_inquiry, sales_order, sales_review, supplier,
    supplier_api, supplier_offering,
};
use mongodb::Client;
use mongodb::bson::doc;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{Executor, NoTransaction, ReadExecutor};

use super::super::facilities::Facilities;
use super::fixtures::{AuthHarness, actor, company_scope, complete, object, without_index};
use crate::entity::access_control::ScopeClause;
use crate::entity::authorization::{AccessContext, AccessTarget, FactValue, ObjectFacts, ResourceTargets};
use crate::entity::authorization_bundle::plan::{PolicyFacts, PolicyPlan};
use crate::entity::authorization_bundle::{
    PolicyBinding, PolicyDocument, PolicyMode, PolicyRole, PolicyScope, PolicyVersion,
};
use crate::entity::person_profile_change::ORGANIZATION_PERSON_PROFILE_CHANGE;
use crate::entity::rbac::PermissionSet;
use crate::entity::resource_access::event::GrantEvent;
use crate::entity::resource_access::{GrantTarget, ResourceKind, SourceUpdate};
use crate::ports::ResourceGrantFacts;
use crate::repository::authorization::fixtures::execute;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::service::iam::management::ManagementAccess;
use crate::service::resource_access::ResourceGrantService;
use crate::{Error, QualifiedPermissions, Result, RoleId};

// ── 访问检查 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scoped_inspection_requires_complete_same_role_qualification() {
    let h = AuthHarness::new();
    let perm = sales_inquiry::COMMANDS_SALES_CREATE;
    for roles in [
        vec![("creator", &[perm.clone()][..])],
        vec![("creator", &[perm.clone()][..]), ("catalog", &[catalog::PRODUCT_SELLABLE_SKU_LIST][..])],
    ] {
        h.grants(&roles);
        assert!(matches!(
            h.service.inspection_scope(&actor(), &perm, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }
    h.grants(&[("creator", &[perm.clone(), catalog::PRODUCT_SELLABLE_SKU_LIST])]);
    let scope =
        h.service.inspection_scope(&actor(), &perm, &mut SnapshotExecutor::default()).await.unwrap().unwrap();
    assert!(scope.scope.has_role_scope());
}

#[tokio::test]
async fn permission_source_uses_current_target_roles_and_fixed_origin() {
    let h = AuthHarness::new();
    h.grants(&[("contract-reader", &[contract::DETAIL]), ("customer-reader", &[customer::LIST])]);
    h.set_role_scope(
        "contract-reader",
        &customer::LIST,
        ScopeClause { self_owned: true, ..Default::default() },
    );
    h.set_role_scope("customer-reader", &customer::LIST, ScopeClause { company: true, ..Default::default() });
    h.set_role_scope(
        "contract-reader",
        &contract::DETAIL,
        ScopeClause { company: true, ..Default::default() },
    );
    let access = h
        .service
        .inspection_scope(&actor(), &contract::DETAIL, &mut SnapshotExecutor::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.scope.role_clauses, vec![ScopeClause { self_owned: true, ..Default::default() }]);
}

#[tokio::test]
async fn contextual_inspection_qualifies_all_permissions_without_object_approval() {
    let h = AuthHarness::new();
    for (perm, companion) in [
        (cost::ENTRY_LIST, sales_order::LIST),
        (approval_instance::SUBMIT_DECISION, approval_instance::INSTANCE_LIST),
    ] {
        h.grants(&[("role", std::slice::from_ref(&perm))]);
        assert!(matches!(
            h.service.inspection_scope(&actor(), &perm, &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
        h.grants(&[("role", &[perm.clone(), companion])]);
        assert!(
            h.service
                .inspection_scope(&actor(), &perm, &mut SnapshotExecutor::default())
                .await
                .unwrap()
                .is_none()
        );
    }

    h.grants(&[("inspector", &[access_control::INSPECTION_INSPECT])]);
    assert!(matches!(
        h.service
            .inspection_scope(&actor(), &access_control::INSPECTION_INSPECT, &mut SnapshotExecutor::default())
            .await,
        Err(Error::Forbidden(_))
    ));
    h.grants(&[(
        "inspector",
        &[
            access_control::INSPECTION_INSPECT,
            admin::ACCOUNT_LIST_ADMINS,
            admin::ROLE_LIST_ROLES,
            organization::LIST,
        ],
    )]);
    assert!(
        h.service
            .inspection_scope(&actor(), &access_control::INSPECTION_INSPECT, &mut SnapshotExecutor::default())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn inspection_preserves_identity_denials_and_facility_errors() {
    let h = AuthHarness::new();
    h.set_active(false);
    assert!(matches!(
        h.service.inspection_scope(&actor(), &contract::DETAIL, &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));
    h.set_fail(true);
    assert!(matches!(
        h.service.inspection_scope(&actor(), &contract::DETAIL, &mut SnapshotExecutor::default()).await,
        Err(Error::Internal(_))
    ));
    h.set_active(true);
    h.set_fail(false);
    let unknown = Permission::parse("unknown:list").unwrap();
    assert!(matches!(
        h.service.inspection_scope(&actor(), &unknown, &mut SnapshotExecutor::default()).await,
        Err(Error::Forbidden(_))
    ));
}

// ── 授权文件 ──────────────────────────────────────────────────────────────────

fn empty_policy_doc() -> PolicyDocument {
    PolicyDocument { version: PolicyVersion::V1, roles: vec![], bindings: vec![], data_scopes: vec![] }
}

fn configure_bundle(h: &AuthHarness, perm: &Permission, roles: &[(&str, &[Permission])]) {
    h.grants(roles);
    let source =
        if perm == &access_control::POLICY_BUNDLE_EXPORT { organization::LIST } else { organization::CHANGE };
    h.set_company_scope(perm, &source);
    h.set_company_scope(&source, &source);
}

async fn check_document_requirements(h: &AuthHarness, doc: &PolicyDocument, required: &[Permission]) {
    for perm in [access_control::POLICY_BUNDLE_PREVIEW, access_control::POLICY_BUNDLE_APPLY] {
        configure_bundle(h, &perm, &[("manager", required)]);
        h.service
            .require_policy_bundle(&actor(), &perm, Some(doc), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        for missing in 0..required.len() {
            let partial = without_index(required, missing);
            for split in [false, true] {
                let mut roles = vec![("partial", partial.as_slice())];
                if split {
                    roles.push(("separate", &required[missing..missing + 1]));
                }
                configure_bundle(h, &perm, &roles);
                assert!(matches!(
                    h.service
                        .require_policy_bundle(&actor(), &perm, Some(doc), &mut SnapshotExecutor::default())
                        .await,
                    Err(Error::Forbidden(_))
                ));
            }
        }
    }
}

#[tokio::test]
async fn documents_require_the_entire_same_role_combination_for_their_actual_write_kinds() {
    let h = AuthHarness::new();
    let mut doc = empty_policy_doc();
    let mut required = vec![
        access_control::POLICY_BUNDLE_PREVIEW,
        access_control::POLICY_BUNDLE_APPLY,
        organization::CHANGE,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
        access_control::PERSON_SCOPE_LIST,
    ];
    check_document_requirements(&h, &doc, &required).await;

    doc.roles.push(PolicyRole {
        id: RoleId::parse("role-test").unwrap(),
        name: "测试岗位".into(),
        mode: PolicyMode::Merge,
        permissions: vec![],
    });
    required.extend([admin::ROLE_CREATE_ROLE, admin::ROLE_UPDATE_ROLE]);
    check_document_requirements(&h, &doc, &required).await;

    doc.bindings.push(PolicyBinding {
        user_id: "target".into(),
        mode: PolicyMode::Replace,
        role_ids: vec![],
    });
    required.push(admin::ACCOUNT_UPDATE_ADMIN);
    check_document_requirements(&h, &doc, &required).await;

    doc.data_scopes.push(PolicyScope {
        user_id: "target".into(),
        resource: "customer".into(),
        actions: vec!["list".into()],
        mode: PolicyMode::Replace,
        grants: vec![],
    });
    required.push(access_control::PERSON_SCOPE_SAVE);
    check_document_requirements(&h, &doc, &required).await;
}

#[tokio::test]
async fn export_and_public_operations_share_the_fixed_company_boundary() {
    let h = AuthHarness::new();
    let perm = &access_control::POLICY_BUNDLE_EXPORT;
    let required = [
        access_control::POLICY_BUNDLE_EXPORT,
        organization::LIST,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
        access_control::PERSON_SCOPE_LIST,
    ];
    configure_bundle(&h, perm, &[("manager", &required)]);
    h.service.require_policy_bundle(&actor(), perm, None, &mut SnapshotExecutor::default()).await.unwrap();
    h.require_snap(perm, AccessTarget::Operation).await.unwrap();

    for company in [true, false] {
        for key in [(perm.clone(), organization::LIST), (organization::LIST, organization::LIST)] {
            h.identity.scopes.lock().unwrap().get_mut(&key).unwrap().user_limit =
                Some(ScopeClause { company, self_owned: !company, ..Default::default() });
        }
        assert_eq!(
            h.service
                .require_policy_bundle(&actor(), perm, None, &mut SnapshotExecutor::default())
                .await
                .is_ok(),
            company
        );
        assert_eq!(h.require_snap(perm, AccessTarget::Operation).await.is_ok(), company);
    }

    configure_bundle(&h, perm, &[("entry-only", std::slice::from_ref(perm))]);
    assert!(matches!(h.require_snap(perm, AccessTarget::Operation).await, Err(Error::Forbidden(_))));

    configure_bundle(&h, perm, &[("manager", &required)]);
    assert!(matches!(
        h.service.require_policy_bundle(&actor(), perm, None, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        h.service
            .require_policy_bundle(
                &actor(),
                perm,
                Some(&empty_policy_doc()),
                &mut SnapshotExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn preview_and_apply_preserve_the_same_authorized_target_and_review_hash() {
    let h = AuthHarness::new();
    let required = [
        access_control::POLICY_BUNDLE_PREVIEW,
        access_control::POLICY_BUNDLE_APPLY,
        organization::CHANGE,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
        access_control::PERSON_SCOPE_LIST,
    ];
    let doc = empty_policy_doc();
    let mut hashes = Vec::new();
    for perm in [access_control::POLICY_BUNDLE_PREVIEW, access_control::POLICY_BUNDLE_APPLY] {
        configure_bundle(&h, &perm, &[("manager", &required)]);
        let access = h
            .service
            .require_policy_bundle(&actor(), &perm, Some(&doc), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        let (_, preview) = PolicyPlan::build(
            doc.clone(),
            &PolicyFacts::default(),
            actor().id(),
            &PermissionSet::new(required.clone()),
            access.policy_version,
            &access.scope_version,
        )
        .unwrap();
        hashes.push(preview.review_hash);
    }
    assert_eq!(hashes[0], hashes[1]);
}

// ── 管理合同、审批定义与纯功能入口 ──────────────────────────────────────────

fn def_candidate(subject: &str) -> AccessTarget {
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
    async fn compute_source(&self, _: &GrantEvent, _: &mut dyn Executor) -> Result<SourceUpdate> {
        panic!("管理员资格检查不得读取自动来源")
    }
}

#[tokio::test]
async fn record_grant_management_uses_the_complete_shared_scope_qualification() {
    let h = AuthHarness::new();
    let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("resource_manager_unit");
    let manager = ResourceGrantService::new(db, Arc::new(h.service), Arc::new(NoGrantFacts));
    let required = [
        access_control::RESOURCE_GRANT_MANAGE,
        organization::CHANGE,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
    ];
    let source = (access_control::RESOURCE_GRANT_MANAGE, organization::CHANGE);
    h.identity.scopes.lock().unwrap().insert(source.clone(), company_scope());
    h.identity.state.lock().unwrap().grants = vec![("manager".into(), required.to_vec())];
    manager.authorize_manager(&actor(), &mut SnapshotExecutor::default()).await.unwrap();

    for roles in [
        vec![("legacy-manager".into(), required[..2].to_vec())],
        vec![("scope".into(), required[..2].to_vec()), ("directory".into(), required[2..].to_vec())],
    ] {
        h.identity.state.lock().unwrap().grants = roles;
        assert!(matches!(
            manager.authorize_manager(&actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
    }

    h.identity.state.lock().unwrap().grants = vec![("manager".into(), required.to_vec())];
    h.identity.scopes.lock().unwrap().get_mut(&source).unwrap().user_limit =
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
        let h = AuthHarness::new();
        let required = [admin::ACCOUNT_LIST_ADMINS, admin::ROLE_LIST_ROLES, entry.clone(), source.clone()];
        h.set_company_scope(&entry, &source);
        h.grants(&[("manager", &required)]);
        h.require_snap(&entry, AccessTarget::Operation).await.unwrap();

        h.grants(&[("entry", &required[..3]), ("organization", &required[3..])]);
        assert!(matches!(h.require_snap(&entry, AccessTarget::Operation).await, Err(Error::Forbidden(_))));

        h.grants(&[("manager", &required)]);
        h.identity.scopes.lock().unwrap().get_mut(&(entry.clone(), source)).unwrap().user_limit =
            Some(ScopeClause { self_owned: true, ..Default::default() });
        assert!(matches!(h.require_snap(&entry, AccessTarget::Operation).await, Err(Error::Forbidden(_))));
        assert!(matches!(
            h.require_read(&entry, AccessTarget::Operation).await,
            Err(Error::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn definition_mutations_require_entry_and_actual_type_before_allowing_the_candidate() {
    let h = AuthHarness::new();
    for entry in [
        approval_process::CREATE_DEFINITION_DRAFT,
        approval_process::REPLACE_DEFINITION_NODES,
        approval_process::PUBLISH_DEFINITION,
        approval_process::RETIRE_DEFINITION,
    ] {
        for roles in [
            vec![("type", &[approval_admin::SALES_ORDER_DEFINITION][..])],
            vec![("entry", std::slice::from_ref(&entry))],
        ] {
            h.grants(&roles);
            assert!(matches!(
                h.require_snap(&entry, def_candidate("sales_order")).await,
                Err(Error::Forbidden(_))
            ));
        }
        h.grants(&[
            ("entry", std::slice::from_ref(&entry)),
            ("type", &[approval_admin::SALES_ORDER_DEFINITION]),
        ]);
        h.require_snap(&entry, def_candidate("sales_order")).await.unwrap();
        assert!(matches!(
            h.require_snap(&entry, def_candidate("purchase_order")).await,
            Err(Error::Forbidden(_))
        ));
        assert!(matches!(
            h.require_snap(&entry, AccessTarget::Operation).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            h.require_snap(&entry, def_candidate("unknown")).await,
            Err(Error::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn runtime_admin_can_read_a_definition_but_cannot_edit_or_choose_definition_assignees() {
    let h = AuthHarness::new();
    h.grants(&[
        ("entry", &[approval_process::DEFINITION_DETAIL]),
        ("type", &[approval_admin::SALES_ORDER_RUNTIME]),
    ]);
    h.require_snap(&approval_process::DEFINITION_DETAIL, def_candidate("sales_order")).await.unwrap();
    assert!(matches!(
        h.require_snap(&internal::APPROVAL_DEFINITION_ASSIGNEES, def_candidate("sales_order")).await,
        Err(Error::Forbidden(_))
    ));

    h.grants(&[
        ("entry", &[approval_process::DEFINITION_DETAIL]),
        ("type", &[approval_admin::SALES_ORDER_DEFINITION]),
    ]);
    h.require_snap(&internal::APPROVAL_DEFINITION_ASSIGNEES, def_candidate("sales_order")).await.unwrap();
}

#[tokio::test]
async fn definition_read_candidates_require_snapshot_before_reading_permissions() {
    let h = AuthHarness::new();
    h.grants(&[("reader", &[approval_process::DEFINITION_DETAIL, approval_admin::SALES_ORDER_DEFINITION])]);
    for perm in [approval_process::DEFINITION_DETAIL, internal::APPROVAL_DEFINITION_ASSIGNEES] {
        for ex in [&mut ReadExecutor::default() as &mut dyn Executor, &mut NoTransaction] {
            let calls = h.permission_calls();
            assert!(matches!(
                h.require(&perm, def_candidate("sales_order"), ex).await,
                Err(Error::ValidationError(m)) if m == "审批定义授权必须使用快照事务"
            ));
            assert_eq!(h.permission_calls(), calls);
        }
        h.require_snap(&perm, def_candidate("sales_order")).await.unwrap();
    }
}

#[tokio::test]
async fn expired_definition_candidates_report_data_scope_changed() {
    let h = AuthHarness::new();
    h.grants(&[("reader", &[approval_process::DEFINITION_DETAIL, approval_admin::SALES_ORDER_DEFINITION])]);
    for perm in [approval_process::DEFINITION_DETAIL, internal::APPROVAL_DEFINITION_ASSIGNEES] {
        let AccessTarget::Candidate(mut obj) = def_candidate("sales_order") else { unreachable!() };
        obj.valid_until = Some(Instant::from_unix_secs(1));
        assert!(matches!(
            h.require_snap(&perm, AccessTarget::Candidate(obj)).await,
            Err(Error::ConflictError(m)) if m.starts_with("DATA_SCOPE_CHANGED")
        ));
    }
}

#[tokio::test]
async fn definition_permissions_crossing_read_stages_report_data_scope_changed() {
    struct AdvancingIdentity;
    #[async_trait]
    impl Facilities for AdvancingIdentity {
        async fn permissions(
            &self,
            _: &AuditActor,
            p: &[Permission],
            ex: &mut dyn Executor,
        ) -> Result<Option<QualifiedPermissions>> {
            ex.next_read();
            Ok(Some(QualifiedPermissions::test_grants(&[("role", p)])))
        }
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
        async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
            panic!("审批定义候选不得读取记录授权版本")
        }
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
    let mut builder = complete();
    builder.facilities = Arc::new(AdvancingIdentity);
    let service = builder.build().unwrap();
    for perm in [approval_process::DEFINITION_DETAIL, internal::APPROVAL_DEFINITION_ASSIGNEES] {
        assert!(matches!(
            service.require(&actor(), &perm, def_candidate("sales_order"), &mut SnapshotExecutor::default()).await,
            Err(Error::ConflictError(m)) if m.starts_with("DATA_SCOPE_CHANGED")
        ));
    }
}

#[tokio::test]
async fn transfer_prechecks_use_fixed_independent_qualifications_without_internal_grants() {
    let h = AuthHarness::new();
    h.grants(&[
        ("buyer", &[sales_inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES]),
        ("catalog", &[catalog::PRODUCT_SELLABLE_SKU_LIST]),
        ("supplier", &[supplier_offering::LIST]),
    ]);
    h.require_snap(&internal::DATA_TRANSFER_SUPPLIER_QUOTES, AccessTarget::Operation).await.unwrap();

    h.identity.state.lock().unwrap().grants.pop();
    assert!(matches!(
        h.require_snap(&internal::DATA_TRANSFER_SUPPLIER_QUOTES, AccessTarget::Operation).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        h.require_snap(&internal::DATA_TRANSFER_SUPPLIER_QUOTES, def_candidate("sales_order")).await,
        Err(Error::ValidationError(_))
    ));
}

#[tokio::test]
async fn management_entries_and_facility_errors_respect_rechecks_and_snapshot_boundaries() {
    let h = AuthHarness::new();
    let service = Arc::new(h.service);
    let identity = h.identity;
    for perm in [
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
        access_control::AUDIT_EVENT_LIST,
    ] {
        let access = ManagementAccess::new(service.clone(), actor(), &perm);
        identity.state.lock().unwrap().grants = vec![("ordinary".into(), vec![sales_order::LIST])];
        assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
        identity.state.lock().unwrap().grants = vec![("manager".into(), vec![perm.clone()])];
        access.require(&mut SnapshotExecutor::default()).await.unwrap();
        identity.state.lock().unwrap().grants = vec![("manager".into(), vec![])];
        assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
    }

    let access = ManagementAccess::new(service, actor(), &admin::ROLE_CREATE_ROLE);
    identity.state.lock().unwrap().grants = vec![("manager".into(), vec![admin::ROLE_CREATE_ROLE])];
    access.require(&mut SnapshotExecutor::default()).await.unwrap();
    identity.state.lock().unwrap().active = false;
    assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Forbidden(_))));
    identity.state.lock().unwrap().active = true;
    identity.state.lock().unwrap().fail = true;
    assert!(matches!(access.require(&mut SnapshotExecutor::default()).await, Err(Error::Internal(_))));
    let calls = identity.permissions_calls.load(std::sync::atomic::Ordering::SeqCst);
    assert!(matches!(access.require(&mut ReadExecutor::default()).await, Err(Error::ValidationError(_))));
    assert_eq!(identity.permissions_calls.load(std::sync::atomic::Ordering::SeqCst), calls);
}

#[tokio::test]
async fn adoption_materials_change_entries_and_function_reads_preserve_fixed_qualifications() {
    let h = AuthHarness::with_facts("sales_inquiry", vec![]);
    // 目录采纳准备
    for p in [supplier_api::ADOPTIONS_PREVIEW, internal::SUPPLIER_OFFERING_COST_DETAIL] {
        h.grants(&[("incomplete", std::slice::from_ref(&p))]);
        assert!(matches!(
            h.require_snap(&internal::CATALOG_ADOPTION, AccessTarget::Operation).await,
            Err(Error::Forbidden(_))
        ));
    }
    h.grants(&[
        ("catalog", &[supplier_api::ADOPTIONS_PREVIEW]),
        ("cost", &[internal::SUPPLIER_OFFERING_COST_DETAIL]),
    ]);
    h.require_snap(&internal::CATALOG_ADOPTION, AccessTarget::Operation).await.unwrap();
    assert!(matches!(
        h.require_read(&internal::CATALOG_ADOPTION, AccessTarget::Operation).await,
        Err(Error::ValidationError(_))
    ));

    // 客户往来资料字段权限
    for (perm, code) in [
        (party::CONTACT_DETAIL, "party_contact:detail"),
        (party::CONTACT_REVEAL, "party_contact:reveal"),
        (party::ADDRESS_DETAIL, "party_address:detail"),
        (party::ADDRESS_REVEAL, "party_address:reveal"),
        (party::BANK_ACCOUNT_DETAIL, "party_bank_account:detail"),
        (party::TAX_PROFILE_DETAIL, "party_tax_profile:detail"),
    ] {
        assert_eq!(perm, Permission::parse(code).unwrap());
        h.grants(&[("reader", &[party::CONTACT_LIST])]);
        assert!(matches!(h.require_snap(&perm, AccessTarget::Operation).await, Err(Error::Forbidden(_))));
        h.grants(&[("qualified", std::slice::from_ref(&perm))]);
        h.require_snap(&perm, AccessTarget::Operation).await.unwrap();
        assert!(matches!(
            h.require_read(&perm, AccessTarget::Operation).await,
            Err(Error::ValidationError(_))
        ));
    }
    assert_eq!(h.calls("sales_inquiry"), 0);

    // 订单读取不能代替变更单入口
    h.grants(&[(
        "orders",
        &[sales_order::LIST, sales_order::DETAIL, purchase_order::LIST, purchase_order::DETAIL],
    )]);
    for perm in [
        sales_review::SALES_CHANGE_ORDER_LIST,
        sales_review::SALES_CHANGE_ORDER_DETAIL,
        purchase_order::PURCHASE_CHANGE_LIST,
        purchase_order::PURCHASE_CHANGE_DETAIL,
    ] {
        assert!(matches!(h.require_snap(&perm, AccessTarget::Operation).await, Err(Error::Forbidden(_))));
        h.identity.state.lock().unwrap().grants.push(("entry".into(), vec![perm.clone()]));
        h.require_snap(&perm, AccessTarget::Operation).await.unwrap();
    }

    // 纯功能读取无需查询绑定
    let map = complete().mappings[&customer::LIST].clone();
    for perm in [
        customer::ASSIGNMENT_LIST,
        demo_master_data::STATUS,
        supplier_api::CONNECTION_LIST,
        internal::CONNECTION_BIND_ENDPOINT,
        supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS,
    ] {
        assert!(matches!(complete().mapping(&perm, map.clone()), Err(Error::ValidationError(_))));
        h.clear_grants();
        assert!(matches!(h.require_snap(&perm, AccessTarget::Operation).await, Err(Error::Forbidden(_))));
        h.grants(&[("reader", std::slice::from_ref(&perm))]);
        h.require_snap(&perm, AccessTarget::Operation).await.unwrap();
        assert!(matches!(
            h.query(&perm, &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
        h.set_active(false);
        assert!(matches!(h.require_snap(&perm, AccessTarget::Operation).await, Err(Error::Forbidden(_))));
        h.set_active(true);
    }
}

#[tokio::test]
async fn attachment_preparations_keep_fixed_function_qualification_and_reject_object_claims() {
    let cases = [
        (internal::CATALOG_CREATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_PRODUCT_CREATE]),
        (internal::CATALOG_UPDATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_PRODUCT_UPDATE]),
        (internal::CATALOG_BRAND_CREATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_BRAND_CREATE]),
        (internal::CATALOG_BRAND_UPDATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_BRAND_UPDATE]),
        (internal::SUPPLIER_CREATE_ATTACHMENT_UPLOAD, vec![supplier::PROFILE_CREATE]),
        (internal::SUPPLIER_UPDATE_ATTACHMENT_UPLOAD, vec![supplier::PROFILE_UPDATE]),
        (
            internal::PURCHASE_EXPENSE_ATTACHMENT_UPLOAD,
            vec![purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM],
        ),
        (
            internal::ELECTRONIC_DELIVERY_ATTACHMENT_UPLOAD,
            vec![
                fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
                fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
            ],
        ),
        (
            internal::SERVICE_FULFILLMENT_ATTACHMENT_UPLOAD,
            vec![
                fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
                fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
            ],
        ),
        (internal::FINANCE_UPLOAD_CUSTOMER_RECEIPT, vec![receivable::CUSTOMER_RECEIPT_SUBMIT]),
        (internal::FINANCE_UPLOAD_INVOICE_DRAFT, vec![receivable::INVOICE_CREATE]),
        (internal::FINANCE_UPLOAD_INVOICE_POST, vec![receivable::INVOICE_POST]),
        (internal::FINANCE_UPLOAD_PURCHASE_INVOICE, vec![payable::PURCHASE_INVOICE_ALLOCATION_POST]),
        (internal::FINANCE_UPLOAD_EXTRA_COST, vec![cost::ENTRY_CREATE, sales_order::DETAIL]),
        (
            internal::FINANCE_UPLOAD_PURCHASE_PAYMENT,
            vec![purchase_payment_request::CREDIT_LIST, purchase_payment_request::DETAIL],
        ),
    ];
    let h = AuthHarness::new();
    for (entry, required) in cases {
        h.assert_same_role_required(&[], &required, &entry, || AccessTarget::Operation).await;
        h.grants(&[("business", &required)]);
        assert!(matches!(
            h.require_snap(&entry, AccessTarget::record("object").unwrap()).await,
            Err(Error::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn customer_import_source_requires_the_submitter_and_current_create_permission() {
    let h = AuthHarness::with_facts("background_job", vec![object("background_job", "job", "user")]);
    let permission = internal::CUSTOMER_IMPORT_JOB_READ;
    let target = || AccessTarget::record("job").unwrap();

    h.grants(&[("sales", &[customer::LIST])]);
    assert!(matches!(h.require_snap(&permission, target()).await, Err(Error::Forbidden(_))));
    h.grants(&[("sales", &[customer::CREATE])]);
    h.require_snap(&permission, target()).await.unwrap();

    h.mutate_first("background_job", |o| o.owner_id = FactValue::Known(Some("other".into())));
    h.grants(&[("role-sysadmin", &[customer::CREATE, bulk_job::BACKGROUND_JOB_DETAIL])]);
    assert!(matches!(h.require_snap(&permission, target()).await, Err(Error::Forbidden(_))));
}

#[tokio::test]
async fn background_jobs_copy_detail_and_organization_scope_keep_boundaries() {
    // 后台任务管理员与导入读取、取消全部任务
    let h = AuthHarness::with_facts("background_job", vec![object("background_job", "job", "other")]);
    h.grants(&[("role-sysadmin", &[bulk_job::BACKGROUND_JOB_DETAIL, catalog::PRODUCT_PRODUCT_LIST])]);
    h.require_snap(&bulk_job::BACKGROUND_JOB_DETAIL, AccessTarget::record("job").unwrap()).await.unwrap();
    assert!(matches!(
        h.require_snap(&internal::PRODUCT_IMPORT_JOB_READ, AccessTarget::record("job").unwrap()).await,
        Err(Error::Forbidden(_))
    ));
    h.mutate_first("background_job", |o| o.owner_id = FactValue::Known(Some("user".into())));
    h.require_snap(&internal::PRODUCT_IMPORT_JOB_READ, AccessTarget::record("job").unwrap()).await.unwrap();
    h.grants(&[("ordinary", &[bulk_job::BACKGROUND_JOB_DETAIL])]);
    h.mutate_first("background_job", |o| o.owner_id = FactValue::Known(Some("other".into())));
    assert!(matches!(
        h.require_snap(&bulk_job::BACKGROUND_JOB_DETAIL, AccessTarget::record("job").unwrap()).await,
        Err(Error::Forbidden(_))
    ));

    for (role, perms, allowed) in [
        ("ordinary", &[bulk_job::BACKGROUND_JOB_CANCEL_ALL][..], false),
        ("role-sysadmin", &[][..], false),
        ("role-sysadmin", &[bulk_job::BACKGROUND_JOB_CANCEL_ALL][..], true),
    ] {
        h.grants(&[(role, perms)]);
        assert_eq!(
            h.require_snap(&bulk_job::BACKGROUND_JOB_CANCEL_ALL, AccessTarget::Operation).await.is_ok(),
            allowed
        );
    }

    // 抄送详情过滤接收人并结束原阶段
    let h = AuthHarness::with_facts("approval_copy", vec![]);
    h.grants(&[("active", &[])]);
    let mut ex = SnapshotExecutor::default();
    let access = h.query(&internal::APPROVAL_COPY_DETAIL, &mut ex).await.unwrap();
    let rows = execute(
        vec![
            doc! {"id":"mine","owner":"user","deleted_at":0},
            doc! {"id":"other","owner":"another","deleted_at":0},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    assert_eq!(rows.iter().map(|r| r.get_str("id").unwrap()).collect::<Vec<_>>(), ["mine"]);
    access.ensure_current(&mut ex).unwrap();
    h.validate(&access, &mut ex).await.unwrap();
    assert!(access.ensure_current(&mut ex).is_err());

    // 组织档案变更范围与快照阶段
    let h = AuthHarness::with_facts("org_unit", vec![]);
    let mut ex = SnapshotExecutor::default();
    h.grants(&[("org", &[organization::CHANGE]), ("account", &[admin::ACCOUNT_UPDATE_ADMIN])]);
    assert!(matches!(
        h.service.organization_scope(&actor(), &ORGANIZATION_PERSON_PROFILE_CHANGE, &mut ex).await,
        Err(Error::Forbidden(_))
    ));
    h.grants(&[("combined", &[organization::CHANGE, admin::ACCOUNT_UPDATE_ADMIN])]);
    let scope =
        h.service.organization_scope(&actor(), &ORGANIZATION_PERSON_PROFILE_CHANGE, &mut ex).await.unwrap();
    scope.ensure_current(&mut ex).unwrap();
    ex.next_read();
    assert!(scope.ensure_current(&mut ex).is_err());
    assert!(
        h.service
            .organization_scope(&actor(), &organization::CHANGE, &mut ReadExecutor::default())
            .await
            .is_err()
    );
    assert!(h.service.organization_scope(&actor(), &customer::LIST, &mut ex).await.is_err());
}
