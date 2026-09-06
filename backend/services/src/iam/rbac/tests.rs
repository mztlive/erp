use std::collections::{HashMap, HashSet};

use casbin::{CoreApi, Enforcer, MemoryAdapter, RbacApi};
use entities::{Permission, PermissionSet, Role, RoleData};
use erp_core::AccountKind;

use super::{
    collect_role_ids, collect_role_permissions, commit_outcome_unknown, ensure_all_roles_assignable,
    ensure_management_subset, ensure_permission_subset, ensure_policy_snapshot_revision,
    ensure_role_deletable, ensure_role_mutable, ensure_roles_delegable, ensure_target_roles_manageable,
    parse_policy_permissions, permission_pairs, permissions_for_roles, policy_revisions_match, role_key,
    role_or_not_found, root_role_is_current, stable_policy_revision, RbacService, RolePermissionSnapshot,
    RBAC_MODEL, ROOT_ROLE_ID,
};
use crate::errors::Error;

#[test]
fn commit_outcome_unknown_is_detected_separately_from_definite_errors() {
    let unknown = Error::from(persistence_core::Error::CommitOutcomeUnknown(
        mongodb::error::Error::custom("unknown"),
    ));
    let definite = Error::NotFound("role".to_string());

    assert!(commit_outcome_unknown(&unknown));
    assert!(!commit_outcome_unknown(&definite));
}

#[test]
fn policy_snapshot_is_accepted_only_when_revision_stays_stable() {
    assert_eq!(stable_policy_revision(7, 7), Some(7));
    assert_eq!(stable_policy_revision(7, 8), None);
}

/// 角色绑定与逐角色授权必须来自同一个可验证 revision，禁止 R/R+1 拼接。
#[test]
fn role_permission_snapshot_keeps_roles_grants_and_revision_together() {
    let permission = Permission::parse("approval_instance:decide").unwrap();
    let read = Permission::parse("stock_adjustment:detail").unwrap();
    let create = Permission::parse("stock_adjustment:create").unwrap();
    let snapshot = RolePermissionSnapshot {
        role_ids: vec!["role-a".to_string(), "role-b".to_string(), "role-c".to_string()],
        grants: HashMap::from([
            ("role-a".to_string(), HashSet::from([read.clone()])),
            (
                "role-b".to_string(),
                HashSet::from([permission.clone(), read.clone(), create.clone()]),
            ),
            ("role-c".to_string(), HashSet::from([create.clone()])),
        ]),
        policy_revision: 7,
    };
    assert_eq!(snapshot.role_ids(), &["role-a", "role-b", "role-c"]);
    assert_eq!(snapshot.granting_role_ids(&permission), vec!["role-b"]);
    assert_eq!(
        snapshot.granting_role_ids_for_all(&[read, create]),
        vec!["role-b"]
    );
    assert_eq!(snapshot.policy_revision(), 7);
    assert!(ensure_policy_snapshot_revision(7, 7).is_ok());
    assert!(matches!(
        ensure_policy_snapshot_revision(7, 8),
        Err(Error::Rbac(message)) if message.contains("授权策略版本已变化")
    ));
}

#[test]
fn all_required_permissions_cannot_be_spliced_across_roles() {
    let detail = Permission::parse("stock_adjustment:detail").unwrap();
    let create = Permission::parse("stock_adjustment:create").unwrap();
    let snapshot = RolePermissionSnapshot {
        role_ids: vec!["role-detail".to_string(), "role-create".to_string()],
        grants: HashMap::from([
            ("role-detail".to_string(), HashSet::from([detail.clone()])),
            ("role-create".to_string(), HashSet::from([create.clone()])),
        ]),
        policy_revision: 9,
    };

    assert!(snapshot.granting_role_ids_for_all(&[detail, create]).is_empty());
    assert!(snapshot.granting_role_ids_for_all(&[]).is_empty());
}

#[test]
fn remote_policy_revision_invalidates_the_local_snapshot() {
    assert!(policy_revisions_match(7, 7));
    assert!(!policy_revisions_match(7, 8));
}

#[test]
fn root_initialization_is_noop_only_for_canonical_metadata_and_policy() {
    let root = Permission::parse("*:*").unwrap();
    let mut role = Role::new(
        ROOT_ROLE_ID.to_string(),
        RoleData {
            name: "超级管理员".to_string(),
            description: None,
            system: true,
        },
    )
    .unwrap();
    assert!(root_role_is_current(&role, std::slice::from_ref(&root), &root));
    assert!(!root_role_is_current(&role, &[], &root));
    role.disabled = true;
    assert!(!root_role_is_current(&role, std::slice::from_ref(&root), &root));
    role.disabled = false;
    role.system = false;
    assert!(!root_role_is_current(&role, std::slice::from_ref(&root), &root));
}

#[test]
fn system_role_is_rejected_by_normal_delegation_boundary() {
    let system = Role::new(
        "role-system".to_string(),
        RoleData {
            name: "系统角色".to_string(),
            description: None,
            system: true,
        },
    )
    .unwrap();

    assert!(matches!(
        ensure_roles_delegable(&[system]),
        Err(Error::Forbidden(_))
    ));
}

#[test]
fn root_id_is_protected_even_when_legacy_metadata_is_not_system() {
    let root = Role::new(
        ROOT_ROLE_ID.to_string(),
        RoleData {
            name: "错误元数据".to_string(),
            description: None,
            system: false,
        },
    )
    .unwrap();

    assert!(matches!(
        ensure_roles_delegable(std::slice::from_ref(&root)),
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(ensure_role_mutable(&root), Err(Error::Forbidden(_))));
    assert!(matches!(ensure_role_deletable(&root), Err(Error::Forbidden(_))));
}

#[test]
fn actor_can_grant_only_a_covered_permission_subset() {
    let actor = PermissionSet::new([Permission::parse("customer:*").unwrap()]);
    let allowed = PermissionSet::new([Permission::parse("customer:update").unwrap()]);
    let elevated = PermissionSet::new([Permission::parse("*:*").unwrap()]);

    assert!(ensure_permission_subset(&actor, &allowed).is_ok());
    assert!(matches!(
        ensure_permission_subset(&actor, &elevated),
        Err(Error::Forbidden(_))
    ));
}

#[test]
fn actor_can_manage_only_an_equal_or_lower_permission_target() {
    let actor = PermissionSet::new([Permission::parse("customer:*").unwrap()]);
    let equal = PermissionSet::new([Permission::parse("customer:*").unwrap()]);
    let lower = PermissionSet::new([Permission::parse("customer:update").unwrap()]);
    let higher = PermissionSet::new([Permission::parse("*:*").unwrap()]);

    assert!(ensure_management_subset(&actor, &equal).is_ok());
    assert!(ensure_management_subset(&actor, &lower).is_ok());
    assert!(matches!(
        ensure_management_subset(&actor, &higher),
        Err(Error::Forbidden(_))
    ));
}

#[test]
fn root_and_other_system_role_targets_are_not_manageable() {
    let system = Role::new(
        "role-system".to_string(),
        RoleData {
            name: "系统角色".to_string(),
            description: None,
            system: true,
        },
    )
    .unwrap();
    let roles = [(system.base.id.clone(), system)].into_iter().collect();

    assert!(matches!(
        ensure_target_roles_manageable(&[ROOT_ROLE_ID.to_string()], &Default::default()),
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        ensure_target_roles_manageable(&["role-system".to_string()], &roles),
        Err(Error::Forbidden(_))
    ));
}

#[test]
fn permission_pairs_are_deduplicated_and_sorted() {
    let pairs = permission_pairs(vec![
        Permission::parse("customer:read").unwrap(),
        Permission::parse("role:write").unwrap(),
        Permission::parse("customer:read").unwrap(),
    ]);

    assert_eq!(
        pairs,
        vec![
            ("customer".to_string(), "read".to_string()),
            ("role".to_string(), "write".to_string()),
        ]
    );
}

#[test]
fn loaded_policy_permissions_use_the_same_deduplicated_order() {
    let permissions = parse_policy_permissions(vec![
        vec!["role:a".to_string(), "customer".to_string(), "read".to_string()],
        vec!["role:a".to_string(), "role".to_string(), "write".to_string()],
        vec!["role:a".to_string(), "customer".to_string(), "read".to_string()],
    ])
    .unwrap();

    assert_eq!(
        permissions,
        vec![
            Permission::parse("customer:read").unwrap(),
            Permission::parse("role:write").unwrap(),
        ]
    );
}

#[test]
fn missing_or_disabled_role_is_rejected_before_cas_touch() {
    let result = ensure_all_roles_assignable(2, 1);

    assert!(matches!(
        result,
        Err(Error::BusinessLogicError(message)) if message == "角色不存在或已停用"
    ));
    assert!(ensure_all_roles_assignable(2, 2).is_ok());
}

#[test]
fn missing_role_is_rejected_before_policy_replacement() {
    let result = role_or_not_found(None);

    assert!(matches!(result, Err(Error::NotFound(message)) if message == "角色不存在"));
}

#[tokio::test]
async fn unknown_commit_outcome_poison_policy_state() {
    let client = mongodb::Client::with_uri_str("mongodb://localhost:27017")
        .await
        .unwrap();
    let rbac = RbacService::new(client.database("rbac-unit-test"));
    let unknown = Error::from(persistence_core::Error::CommitOutcomeUnknown(
        mongodb::error::Error::custom("unknown"),
    ));

    let result = rbac.finish_policy_transaction::<()>(Err(unknown)).await;

    assert!(matches!(
        result,
        Err(Error::OutcomeUnknown(
            persistence_core::Error::CommitOutcomeUnknown(_)
        ))
    ));
    assert!(matches!(
        rbac.ensure_policy_consistency_known(),
        Err(Error::Rbac(_))
    ));
}

#[tokio::test]
async fn casbin_model_should_enforce_role_permissions_and_wildcards() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer
        .add_role_for_user("user:admin:1", &role_key("role-root"), None)
        .await
        .unwrap();
    enforcer
        .add_permission_for_user(&role_key("role-root"), vec!["*".to_string(), "*".to_string()])
        .await
        .unwrap();

    let permission = Permission::parse("role:delete").unwrap();
    assert!(enforcer
        .enforce(("user:admin:1", permission.resource(), permission.action(),))
        .unwrap());
    assert!(!enforcer.enforce(("user:admin:2", "role", "delete")).unwrap());
}

#[tokio::test]
async fn role_grant_scope_includes_inherited_permissions() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer
        .add_role_for_user(&role_key("role-child"), &role_key("role-parent"), None)
        .await
        .unwrap();
    enforcer
        .add_permission_for_user(
            &role_key("role-parent"),
            vec!["customer".to_string(), "delete".to_string()],
        )
        .await
        .unwrap();

    let permissions = permissions_for_roles(&enforcer, &["role-child".to_string()]).unwrap();

    assert_eq!(
        permissions.as_slice(),
        &[Permission::parse("customer:delete").unwrap()]
    );
}

#[tokio::test]
async fn batch_role_ids_should_include_unbound_accounts() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer
        .add_role_for_user("user:admin:1", &role_key("role-a"), None)
        .await
        .unwrap();
    enforcer
        .add_role_for_user("user:admin:1", &role_key("role-b"), None)
        .await
        .unwrap();

    let account_ids = vec!["1".to_string(), "2".to_string()];
    let role_ids = collect_role_ids(&enforcer, AccountKind::Admin, &account_ids);
    let mut bound_role_ids = role_ids.get("1").cloned().unwrap();
    bound_role_ids.sort();

    assert_eq!(bound_role_ids, vec!["role-a".to_string(), "role-b".to_string()]);
    assert!(role_ids.get("2").is_some_and(Vec::is_empty));
}

#[tokio::test]
async fn batch_role_permissions_should_include_roles_without_permissions() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer
        .add_permission_for_user(
            &role_key("role-a"),
            vec!["customer".to_string(), "list".to_string()],
        )
        .await
        .unwrap();

    let role_ids = vec!["role-a".to_string(), "role-b".to_string()];
    let permissions = collect_role_permissions(&enforcer, &role_ids).unwrap();

    assert_eq!(
        permissions.get("role-a"),
        Some(&vec![Permission::parse("customer:list").unwrap()])
    );
    assert!(permissions.get("role-b").is_some_and(Vec::is_empty));
}
