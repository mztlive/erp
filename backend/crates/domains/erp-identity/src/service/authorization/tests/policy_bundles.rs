//! 授权文件通过统一服务执行固定资格及文档附加资格，不连接数据库。
use std::sync::Arc;

use erp_permissions::{access_control, admin, organization};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, service};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::AccessTarget;
use crate::entity::authorization_bundle::plan::{PolicyFacts, PolicyPlan};
use crate::entity::authorization_bundle::{
    PolicyBinding, PolicyDocument, PolicyMode, PolicyRole, PolicyScope, PolicyVersion,
};
use crate::entity::rbac::PermissionSet;
use crate::{AuthorizationService, Error, Permission, RoleId};

// 构造空的当前版本授权文件，供各写入用例添加实际文档内容。
fn document() -> PolicyDocument {
    PolicyDocument { version: PolicyVersion::V1, roles: vec![], bindings: vec![], data_scopes: vec![] }
}

// 按指定动作配置角色资格和公司范围，导出使用组织读取，预览和应用使用组织变更。
fn configure(identity: &Identity, permission: &Permission, grants: Vec<(String, Vec<Permission>)>) {
    identity.state.lock().unwrap().grants = grants;
    let source = if permission == &access_control::POLICY_BUNDLE_EXPORT {
        organization::LIST
    } else {
        organization::CHANGE
    };
    let scope = ResolvedScope {
        role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
        user_limit: None,
    };
    identity
        .scopes
        .lock()
        .unwrap()
        .extend([((permission.clone(), source.clone()), scope.clone()), ((source.clone(), source), scope)]);
}

// 对同一授权文件验证预览与应用，并逐项排除缺权限及跨角色补权限。
async fn check_document_requirements(
    service: &AuthorizationService,
    identity: &Identity,
    document: &PolicyDocument,
    required: &[Permission],
) {
    for permission in [access_control::POLICY_BUNDLE_PREVIEW, access_control::POLICY_BUNDLE_APPLY] {
        configure(identity, &permission, vec![("manager".into(), required.to_vec())]);
        service
            .require_policy_bundle(&actor(), &permission, Some(document), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        for missing in 0..required.len() {
            let partial = required
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != missing)
                .map(|(_, permission)| permission.clone())
                .collect::<Vec<_>>();
            for split in [false, true] {
                let mut roles = vec![("partial".into(), partial.clone())];
                if split {
                    roles.push(("separate".into(), vec![required[missing].clone()]));
                }
                configure(identity, &permission, roles);
                assert!(matches!(
                    service
                        .require_policy_bundle(
                            &actor(),
                            &permission,
                            Some(document),
                            &mut SnapshotExecutor::default(),
                        )
                        .await,
                    Err(Error::Forbidden(_))
                ));
            }
        }
    }
}

// 授权文件按实际涉及的写入类型检查完整同角色资格。
#[tokio::test]
async fn documents_require_the_entire_same_role_combination_for_their_actual_write_kinds() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let mut document = document();
    let mut required = vec![
        access_control::POLICY_BUNDLE_PREVIEW,
        access_control::POLICY_BUNDLE_APPLY,
        organization::CHANGE,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
        access_control::PERSON_SCOPE_LIST,
    ];
    check_document_requirements(&service, &identity, &document, &required).await;

    document.roles.push(PolicyRole {
        id: RoleId::parse("role-test").unwrap(),
        name: "测试岗位".into(),
        mode: PolicyMode::Merge,
        permissions: vec![],
    });
    required.extend([admin::ROLE_CREATE_ROLE, admin::ROLE_UPDATE_ROLE]);
    check_document_requirements(&service, &identity, &document, &required).await;

    document.bindings.push(PolicyBinding {
        user_id: "target".into(),
        mode: PolicyMode::Replace,
        role_ids: vec![],
    });
    required.push(admin::ACCOUNT_UPDATE_ADMIN);
    check_document_requirements(&service, &identity, &document, &required).await;

    document.data_scopes.push(PolicyScope {
        user_id: "target".into(),
        resource: "customer".into(),
        actions: vec!["list".into()],
        mode: PolicyMode::Replace,
        grants: vec![],
        replace_legacy: false,
    });
    required.push(access_control::PERSON_SCOPE_SAVE);
    check_document_requirements(&service, &identity, &document, &required).await;
}

// 授权文件导出和公开操作都遵守同一固定公司范围。
#[tokio::test]
async fn export_and_public_operations_share_the_fixed_company_boundary() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let permission = &access_control::POLICY_BUNDLE_EXPORT;
    let required = vec![
        access_control::POLICY_BUNDLE_EXPORT,
        organization::LIST,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
        access_control::PERSON_SCOPE_LIST,
    ];
    configure(&identity, permission, vec![("manager".into(), required.clone())]);
    service
        .require_policy_bundle(&actor(), permission, None, &mut SnapshotExecutor::default())
        .await
        .unwrap();
    service
        .require(&actor(), permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
        .await
        .unwrap();

    for company in [true, false] {
        for key in [(permission.clone(), organization::LIST), (organization::LIST, organization::LIST)] {
            identity.scopes.lock().unwrap().get_mut(&key).unwrap().user_limit =
                Some(ScopeClause { company, self_owned: !company, ..Default::default() });
        }
        assert_eq!(
            service
                .require_policy_bundle(&actor(), permission, None, &mut SnapshotExecutor::default())
                .await
                .is_ok(),
            company
        );
        assert_eq!(
            service
                .require(&actor(), permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
                .await
                .is_ok(),
            company
        );
    }

    configure(&identity, permission, vec![("entry-only".into(), vec![permission.clone()])]);
    assert!(matches!(
        service
            .require(&actor(), permission, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await,
        Err(Error::Forbidden(_))
    ));

    configure(&identity, permission, vec![("manager".into(), required)]);
    assert!(matches!(
        service.require_policy_bundle(&actor(), permission, None, &mut ReadExecutor::default()).await,
        Err(Error::ValidationError(_))
    ));
    assert!(matches!(
        service
            .require_policy_bundle(&actor(), permission, Some(&document()), &mut SnapshotExecutor::default())
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 预览和应用使用相同授权目标，并生成相同的审查哈希。
#[tokio::test]
async fn preview_and_apply_preserve_the_same_authorized_target_and_review_hash() {
    let identity = Arc::new(Identity::default());
    let service = service(identity.clone(), Arc::new(Facts::default()));
    let required = vec![
        access_control::POLICY_BUNDLE_PREVIEW,
        access_control::POLICY_BUNDLE_APPLY,
        organization::CHANGE,
        admin::ACCOUNT_LIST_ADMINS,
        admin::ROLE_LIST_ROLES,
        access_control::PERSON_SCOPE_LIST,
    ];
    let document = document();
    let mut hashes = Vec::new();
    for permission in [access_control::POLICY_BUNDLE_PREVIEW, access_control::POLICY_BUNDLE_APPLY] {
        configure(&identity, &permission, vec![("manager".into(), required.clone())]);
        let access = service
            .require_policy_bundle(&actor(), &permission, Some(&document), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        let (_, preview) = PolicyPlan::build(
            document.clone(),
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
