use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use async_trait::async_trait;
use casbin::{CoreApi, Enforcer, MemoryAdapter, RbacApi};
use erp_core::AccountKind;
use persistence_core::Executor;

use super::{
    AuthorizedAccountManagement, RBAC_MODEL, ROOT_ROLE_ID, RbacService, RolePermissionSnapshot,
    collect_role_ids, collect_role_permissions, commit_outcome_unknown, ensure_all_roles_assignable,
    ensure_management_subset, ensure_permission_subset, ensure_policy_snapshot_revision,
    ensure_roles_delegable, ensure_target_roles_manageable, parse_policy_permissions, permission_pairs,
    permissions_for_roles, role_key, role_or_not_found, root_role_is_current,
};
use crate::entity::{Permission, PermissionSet, Role, RoleData};
use crate::error::Error;
use crate::ports::{IdentityAuditPort, PreparedResourceAudit};
use crate::{RoleId, RoleIdSet};

#[test]
fn account_management_grant_uses_the_same_revision_and_distinguishes_empty_replacement() {
    let unchanged = AuthorizedAccountManagement { policy_revision: 7, replacement_roles: None };
    assert!(unchanged.into_role_grant().is_none());

    for roles in [vec![], vec![RoleId::parse("reader").unwrap()]] {
        let roles = RoleIdSet::from(roles);
        let managed =
            AuthorizedAccountManagement { policy_revision: 7, replacement_roles: Some(roles.clone()) };
        let revision = managed.policy_revision();
        let grant = managed.into_role_grant().unwrap();
        assert_eq!(grant.policy_revision(), revision);
        assert_eq!(grant.role_ids, roles);
    }
}

// 为策略事务收尾测试提供无需审计设施的端口替身。
struct NoopAudit;

#[async_trait]
impl IdentityAuditPort for NoopAudit {
    // 测试只走事务收尾；若请求准备资源审计，返回错误以暴露意外调用。
    fn resource_log(
        &self,
        _actor: AuditActor,
        _action: &str,
        _resource_type: &str,
        _resource_id: String,
    ) -> crate::error::Result<PreparedResourceAudit> {
        Err(Error::Internal("noop audit".to_string()))
    }

    // 满足审计端口约定；测试替身直接返回成功，无需写库。
    async fn persist(
        &self,
        _audit: &PreparedResourceAudit,
        _executor: &mut dyn Executor,
    ) -> crate::error::Result<()> {
        Ok(())
    }
}

// 验证错误分类能区分未知提交结果与明确的业务失败。
#[test]
fn commit_outcome_unknown_is_detected_separately_from_definite_errors() {
    let unknown =
        Error::from(persistence_core::Error::CommitOutcomeUnknown(mongodb::error::Error::custom("unknown")));
    let definite = Error::NotFound("role".to_string());

    assert!(commit_outcome_unknown(&unknown));
    assert!(!commit_outcome_unknown(&definite));
}

// 验证同一快照保留角色绑定、逐角色授予和策略版本，拒绝拼接 R 和 R+1 的证据。
#[test]
fn role_permission_snapshot_keeps_roles_grants_and_revision_together() {
    let permission = Permission::parse("approval_instance:decide").unwrap();
    let read = Permission::parse("stock_adjustment:detail").unwrap();
    let create = Permission::parse("stock_adjustment:create").unwrap();
    let snapshot = RolePermissionSnapshot {
        role_ids: vec![
            RoleId::parse("role-a").unwrap(),
            RoleId::parse("role-b").unwrap(),
            RoleId::parse("role-c").unwrap(),
        ],
        grants: HashMap::from([
            (RoleId::parse("role-a").unwrap(), HashSet::from([read.clone()])),
            (
                RoleId::parse("role-b").unwrap(),
                HashSet::from([permission.clone(), read.clone(), create.clone()]),
            ),
            (RoleId::parse("role-c").unwrap(), HashSet::from([create.clone()])),
        ]),
        policy_revision: 7,
    };
    assert_eq!(
        snapshot.role_ids(),
        &[
            RoleId::parse("role-a").unwrap(),
            RoleId::parse("role-b").unwrap(),
            RoleId::parse("role-c").unwrap()
        ]
    );
    assert_eq!(snapshot.granting_role_ids(&permission), &[RoleId::parse("role-b").unwrap()]);
    assert_eq!(snapshot.granting_role_ids_for_all(&[read, create]), &[RoleId::parse("role-b").unwrap()]);
    assert_eq!(snapshot.policy_revision(), 7);

    assert!(ensure_policy_snapshot_revision(7, 7).is_ok());
    assert!(matches!(
        ensure_policy_snapshot_revision(7, 8),
        Err(Error::Rbac(message)) if message.contains("授权策略版本已变化")
    ));
}

// 验证组合资格须由单一角色覆盖，跨角色拼接和空要求均得不到授予角色。
#[test]
fn all_required_permissions_cannot_be_spliced_across_roles() {
    let detail = Permission::parse("stock_adjustment:detail").unwrap();
    let create = Permission::parse("stock_adjustment:create").unwrap();
    let snapshot = RolePermissionSnapshot {
        role_ids: vec![RoleId::parse("role-detail").unwrap(), RoleId::parse("role-create").unwrap()],
        grants: HashMap::from([
            (RoleId::parse("role-detail").unwrap(), HashSet::from([detail.clone()])),
            (RoleId::parse("role-create").unwrap(), HashSet::from([create.clone()])),
        ]),
        policy_revision: 9,
    };

    assert!(snapshot.granting_role_ids_for_all(&[detail, create]).is_empty());
    assert!(snapshot.granting_role_ids_for_all(&[]).is_empty());
}

// 验证经营盈亏复用销售范围时，收入与成本权限仍须由同一角色授予。
#[test]
fn profit_loss_source_scope_still_requires_cost_permission_in_the_same_role() {
    let sales = Permission::parse("sales_order:list").unwrap();
    let cost = Permission::parse("cost_entry:list").unwrap();
    let required = [sales.clone(), cost.clone()];
    let mut snapshot = RolePermissionSnapshot {
        role_ids: vec![
            RoleId::parse("sales").unwrap(),
            RoleId::parse("cost").unwrap(),
            RoleId::parse("finance").unwrap(),
        ],
        grants: HashMap::from([
            (RoleId::parse("sales").unwrap(), HashSet::from([sales.clone()])),
            (RoleId::parse("cost").unwrap(), HashSet::from([cost.clone()])),
            (RoleId::parse("finance").unwrap(), HashSet::from([sales.clone(), cost.clone()])),
        ]),
        policy_revision: 7,
    };
    assert_eq!(snapshot.granting_role_ids_for_all(&required), &[RoleId::parse("finance").unwrap()]);

    snapshot.grants.remove("finance");
    assert!(snapshot.granting_role_ids_for_all(&required).is_empty());

    snapshot.grants.remove("cost");
    assert_eq!(snapshot.granting_role_ids(&sales), &[RoleId::parse("sales").unwrap()]);
    assert!(snapshot.granting_role_ids_for_all(&required).is_empty());
}

// 验证 root 初始化只在角色已启用、标记为系统角色且具备规范的直接权限时跳过修复。
#[test]
fn root_initialization_is_noop_only_for_canonical_metadata_and_policy() {
    let root = Permission::parse("*:*").unwrap();
    let mut role =
        Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超级管理员").with_system(true))
            .unwrap();
    assert!(root_role_is_current(&role, std::slice::from_ref(&root), &root));
    assert!(!root_role_is_current(&role, &[], &root));
    role.disabled = true;
    assert!(!root_role_is_current(&role, std::slice::from_ref(&root), &root));
    role.disabled = false;
    role.system = false;
    assert!(!root_role_is_current(&role, std::slice::from_ref(&root), &root));
}

// 验证普通角色委派拒绝系统角色。
#[test]
fn system_role_is_rejected_by_normal_delegation_boundary() {
    let system =
        Role::new(RoleId::parse("role-system").unwrap(), RoleData::new("系统角色").with_system(true))
            .unwrap();

    assert!(matches!(ensure_roles_delegable(&[system]), Err(Error::Forbidden(_))));
}

// 验证旧角色缺少系统标记时，固定 root ID 仍禁止普通委派、修改和删除。
#[test]
fn root_id_is_protected_even_when_legacy_metadata_is_not_system() {
    let root = Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("错误元数据")).unwrap();

    assert!(matches!(ensure_roles_delegable(std::slice::from_ref(&root)), Err(Error::Forbidden(_))));
    assert!(root.ensure_mutable().is_err());
    assert!(root.ensure_deletable().is_err());
}

// 验证资源通配权限可授予其覆盖的具体权限，授予全局通配权限须有相应资格。
#[test]
fn actor_can_grant_only_a_covered_permission_subset() {
    let actor = PermissionSet::new([Permission::parse("customer:*").unwrap()]);
    let allowed = PermissionSet::new([Permission::parse("customer:update").unwrap()]);
    let elevated = PermissionSet::new([Permission::parse("*:*").unwrap()]);

    assert!(ensure_permission_subset(&actor, &allowed).is_ok());
    assert!(matches!(ensure_permission_subset(&actor, &elevated), Err(Error::Forbidden(_))));
}

// 验证管理对象的权限等于或低于操作人时通过，超出操作人权限时拒绝。
#[test]
fn actor_can_manage_only_an_equal_or_lower_permission_target() {
    let actor = PermissionSet::new([Permission::parse("customer:*").unwrap()]);
    let equal = PermissionSet::new([Permission::parse("customer:*").unwrap()]);
    let lower = PermissionSet::new([Permission::parse("customer:update").unwrap()]);
    let higher = PermissionSet::new([Permission::parse("*:*").unwrap()]);

    assert!(ensure_management_subset(&actor, &equal).is_ok());
    assert!(ensure_management_subset(&actor, &lower).is_ok());
    assert!(matches!(ensure_management_subset(&actor, &higher), Err(Error::Forbidden(_))));
}

// 验证固定 root ID 及其他系统角色均受普通账号角色管理保护。
#[test]
fn root_and_other_system_role_targets_are_not_manageable() {
    let system =
        Role::new(RoleId::parse("role-system").unwrap(), RoleData::new("系统角色").with_system(true))
            .unwrap();
    let roles = [(system.base.id.clone(), system)].into_iter().collect();

    assert!(matches!(
        ensure_target_roles_manageable(&[RoleId::parse(ROOT_ROLE_ID).unwrap()], &Default::default()),
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        ensure_target_roles_manageable(&[RoleId::parse("role-system").unwrap()], &roles),
        Err(Error::Forbidden(_))
    ));
}

// 验证写入策略前，权限被转换为去重且有序的资源和动作对。
#[test]
fn permission_pairs_are_deduplicated_and_sorted() {
    let pairs = permission_pairs(vec![
        Permission::parse("customer:read").unwrap(),
        Permission::parse("role:write").unwrap(),
        Permission::parse("customer:read").unwrap(),
    ]);

    assert_eq!(
        pairs,
        vec![("customer".to_string(), "read".to_string()), ("role".to_string(), "write".to_string()),]
    );
}

// 验证载入的策略行也按同一顺序规范化并去重。
#[test]
fn loaded_policy_permissions_use_the_same_deduplicated_order() {
    let permissions = parse_policy_permissions(vec![
        vec!["role:a".to_string(), "customer".to_string(), "read".to_string()],
        vec!["role:a".to_string(), "role".to_string(), "write".to_string()],
        vec!["role:a".to_string(), "customer".to_string(), "read".to_string()],
    ])
    .unwrap();

    assert_eq!(
        permissions.as_slice(),
        vec![Permission::parse("customer:read").unwrap(), Permission::parse("role:write").unwrap(),]
    );
}

// 验证绑定写入更新角色版本前的数量校验：缺失或停用导致有效角色数量不足时拒绝。
#[test]
fn missing_or_disabled_role_is_rejected_before_cas_touch() {
    let result = ensure_all_roles_assignable(2, 1);

    assert!(matches!(
        result,
        Err(Error::BusinessLogicError(message)) if message == "角色不存在或已停用"
    ));
    assert!(ensure_all_roles_assignable(2, 2).is_ok());
}

// 验证替换角色权限前，缺失角色被统一报告为 `NotFound`。
#[test]
fn missing_role_is_rejected_before_policy_replacement() {
    let result = role_or_not_found(None);

    assert!(matches!(result, Err(Error::NotFound(message)) if message == "角色不存在"));
}

// 验证未知提交会保留原错误并封锁当前服务实例，其他或新建实例的状态保持独立。
#[tokio::test]
async fn unknown_commit_outcome_poison_policy_state() {
    let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
    let rbac = RbacService::new(client.database("rbac-unit-test"), std::sync::Arc::new(NoopAudit));
    let independent = RbacService::new(client.database("rbac-unit-test"), std::sync::Arc::new(NoopAudit));
    let unknown =
        Error::from(persistence_core::Error::CommitOutcomeUnknown(mongodb::error::Error::custom("unknown")));

    let result = rbac.finish_policy_transaction::<(), Error>(Err(unknown)).await;

    assert!(matches!(result, Err(Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(_)))));
    assert!(matches!(rbac.ensure_policy_consistency_known(), Err(Error::Rbac(_))));
    assert!(independent.ensure_policy_consistency_known().is_ok());
    let next_command = RbacService::new(client.database("rbac-unit-test"), std::sync::Arc::new(NoopAudit));
    assert!(next_command.ensure_policy_consistency_known().is_ok());
}

// 验证生产 Casbin 模型向绑定 root 的账号授予跨资源操作，未绑定账号无法使用其通配权限。
#[tokio::test]
async fn casbin_model_should_enforce_role_permissions_and_wildcards() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer.add_role_for_user("user:admin:1", &role_key("role-root"), None).await.unwrap();
    enforcer
        .add_permission_for_user(&role_key("role-root"), vec!["*".to_string(), "*".to_string()])
        .await
        .unwrap();

    for code in [
        "role:delete",
        "product:create",
        "supplier:update",
        "sales_order:submit",
        "stock_adjustment:create",
        "finance_ledger:read",
        "approval_instance:decide",
    ] {
        let permission = Permission::parse(code).unwrap();
        assert!(enforcer.enforce(("user:admin:1", permission.resource(), permission.action())).unwrap());
    }
    assert!(!enforcer.enforce(("user:admin:2", "role", "delete")).unwrap());
}

// 验证计算角色可授予的权限时纳入父角色的继承权限。
#[tokio::test]
async fn role_grant_scope_includes_inherited_permissions() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer.add_role_for_user(&role_key("role-child"), &role_key("role-parent"), None).await.unwrap();
    enforcer
        .add_permission_for_user(&role_key("role-parent"), vec!["customer".to_string(), "delete".to_string()])
        .await
        .unwrap();

    let permissions = permissions_for_roles(&enforcer, &["role-child".to_string()]).unwrap();

    assert_eq!(permissions.as_slice(), &[Permission::parse("customer:delete").unwrap()]);
}

// 验证批量读取角色绑定时，未绑定账号仍在结果中占一项，角色列表为空。
#[tokio::test]
async fn batch_role_ids_should_include_unbound_accounts() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer.add_role_for_user("user:admin:1", &role_key("role-a"), None).await.unwrap();
    enforcer.add_role_for_user("user:admin:1", &role_key("role-b"), None).await.unwrap();

    let account_ids = vec!["1".to_string(), "2".to_string()];
    let role_ids = collect_role_ids(&enforcer, AccountKind::Admin, &account_ids).unwrap();
    let mut bound_role_ids = role_ids.get("1").cloned().unwrap();
    bound_role_ids.sort();

    assert_eq!(bound_role_ids, vec![RoleId::parse("role-a").unwrap(), RoleId::parse("role-b").unwrap()]);
    assert!(role_ids.get("2").is_some_and(Vec::is_empty));
}

// 验证批量读取直接权限时，无权限角色仍返回一个空权限集。
#[tokio::test]
async fn batch_role_permissions_should_include_roles_without_permissions() {
    let model = casbin::DefaultModel::from_str(RBAC_MODEL).await.unwrap();
    let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
    enforcer
        .add_permission_for_user(&role_key("role-a"), vec!["customer".to_string(), "list".to_string()])
        .await
        .unwrap();

    let role_ids = vec!["role-a".to_string(), "role-b".to_string()];
    let permissions = collect_role_permissions(&enforcer, &role_ids).unwrap();

    assert_eq!(
        permissions.get("role-a"),
        Some(&PermissionSet::new([Permission::parse("customer:list").unwrap()]))
    );
    assert!(permissions.get("role-b").is_some_and(PermissionSet::is_empty));
}

// 验证拒绝诊断能区分缺少某项权限和已有各项权限但缺少共同授予角色。
#[test]
fn permission_diagnostics_distinguish_missing_action_from_role_combination() {
    let create = Permission::parse("purchase_order:create").unwrap();
    let submit = Permission::parse("purchase_order:submit").unwrap();
    let snapshot = RolePermissionSnapshot {
        role_ids: vec![RoleId::parse("creator").unwrap(), RoleId::parse("submitter").unwrap()],
        grants: HashMap::from([
            (RoleId::parse("creator").unwrap(), HashSet::from([create.clone()])),
            (RoleId::parse("submitter").unwrap(), HashSet::from([submit.clone()])),
        ]),
        policy_revision: 1,
    };

    assert!(snapshot.granting_role_ids_for_all(&[create.clone(), submit.clone()]).is_empty());
    assert!(snapshot.denial_message(&[create, submit]).contains("同一有效角色"));
    assert_eq!(
        snapshot.denial_message(&[Permission::parse("sales_order:list").unwrap()]),
        "当前账号缺少 sales_order:list 权限"
    );
}
