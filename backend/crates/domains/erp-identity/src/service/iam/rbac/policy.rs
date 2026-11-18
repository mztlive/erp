use std::collections::HashMap;

use application_core::AuditActor;
use casbin::{Enforcer, RbacApi};
use erp_core::AccountKind;

use super::{ROLE_PREFIX, subject};
use crate::entity::{Permission, PermissionSet, Role, RoleId, RoleIdSet};
use crate::error::{Error, Result};

// 把角色 ID 拼成 Casbin 角色主体。
// 返回 `role:` 前缀加角色 ID。
//
// `role_id`：不含 `role:` 前缀的角色 ID。
pub(super) fn role_key(role_id: &str) -> String {
    format!("{ROLE_PREFIX}{role_id}")
}

// 读取角色直接权限，不展开继承。
// 返回去重后的直接权限。
//
// 策略行无法解析为权限时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<PermissionSet> {
    parse_policy_permissions(enforcer.get_permissions_for_user(&role_key(role_id), None))
}

// 读取角色的隐式权限并收成权限集。
// 返回包含继承权限的 `PermissionSet`。
//
// 策略行无法解析为权限时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn implicit_permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&role_key(role_id), None);
    parse_policy_permissions(policies)
}

// 按角色收集直接权限。
// 返回角色 ID 到直接权限的映射。
//
// 任一角色的策略无法解析时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn collect_role_permissions(
    enforcer: &Enforcer,
    role_ids: &[String],
) -> Result<HashMap<String, PermissionSet>> {
    role_ids.iter().map(|role_id| Ok((role_id.clone(), permissions_for_role(enforcer, role_id)?))).collect()
}

// 读取操作人经角色继承得到的权限集。
// 返回操作人的隐式权限集。
//
// 策略行无法解析为权限时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn permissions_for_actor(enforcer: &Enforcer, actor: &AuditActor) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(actor.kind(), actor.id()), None);
    parse_policy_permissions(policies)
}

// 读取账号经角色继承得到的权限集。
// 返回该账号的隐式权限集。
//
// 策略行无法解析为权限时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn permissions_for_account(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_id: &str,
) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
    parse_policy_permissions(policies)
}

// 合并多个角色的隐式权限。
// 返回这些角色隐式权限的并集。
//
// 任一角色的策略无法解析时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn permissions_for_roles(
    enforcer: &Enforcer,
    role_ids: &[impl AsRef<str>],
) -> Result<PermissionSet> {
    let mut permissions = Vec::new();
    for role_id in role_ids {
        permissions.extend(implicit_permissions_for_role(enforcer, role_id.as_ref())?.into_vec());
    }
    Ok(PermissionSet::new(permissions))
}

// 读取账号直接绑定的角色 ID。
// 返回去掉 `role:` 前缀的角色 ID；非角色主体被忽略。
//
// 持久化角色键非法或未规范化时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn role_ids_for_account(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_id: &str,
) -> Result<Vec<RoleId>> {
    Ok(RoleIdSet::from_casbin_role_keys(
        enforcer.get_roles_for_user(&subject(account_kind, account_id), None),
    )?
    .into())
}

// 批量读取同类账号的直接角色绑定。
// 返回账号 ID 到角色 ID 列表的映射；未绑定的账号对应空列表。
//
// 持久化角色键非法或未规范化时返回错误。
//
// `enforcer`：已加载的 Casbin Enforcer。
pub(super) fn collect_role_ids(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_ids: &[String],
) -> Result<HashMap<String, Vec<RoleId>>> {
    account_ids
        .iter()
        .map(|account_id| Ok((account_id.clone(), role_ids_for_account(enforcer, account_kind, account_id)?)))
        .collect()
}

// 把 Casbin 策略行解析为去重后的权限。
// 返回可解析行组成的权限列表；列数不足的行被跳过。
//
// 资源或动作无法组成合法权限时返回错误。
//
// `policies`：Casbin 策略行，列至少包含主体、资源、动作。
pub(super) fn parse_policy_permissions(policies: Vec<Vec<String>>) -> Result<PermissionSet> {
    let mut permissions = Vec::new();
    for policy in policies {
        let [_, resource, action] = policy.as_slice() else {
            continue;
        };
        permissions.push(Permission::parse(format!("{resource}:{action}"))?);
    }
    Ok(PermissionSet::new(permissions))
}

// 把权限展开为 Casbin 的资源、动作对。
// 返回去重后的 `(resource, action)` 列表。
//
// `permissions`：待写入的权限。
pub(super) fn permission_pairs(permissions: Vec<Permission>) -> Vec<(String, String)> {
    PermissionSet::new(permissions)
        .into_vec()
        .into_iter()
        .map(|permission| (permission.resource().to_string(), permission.action().to_string()))
        .collect()
}

// 把缺失角色收成统一的不存在错误。
// 角色存在时返回该角色。
//
// `role` 为 `None` 时返回 `NotFound`。
//
// `role`：仓储读到的可选角色。
pub(super) fn role_or_not_found(role: Option<Role>) -> Result<Role> {
    role.ok_or_else(|| Error::NotFound("角色不存在".to_string()))
}

// 当错误链里有提交结果未知时返回 `true`。
// 自身或 `source` 链上出现 `CommitOutcomeUnknown` 或 `OutcomeUnknown` 时返回 `true`。
pub(super) fn commit_outcome_unknown(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(err) = current {
        if matches!(
            err.downcast_ref::<persistence_core::Error>(),
            Some(persistence_core::Error::CommitOutcomeUnknown(_))
        ) {
            return true;
        }
        if matches!(err.downcast_ref::<Error>(), Some(Error::OutcomeUnknown(_))) {
            return true;
        }
        current = err.source();
    }
    false
}

// 当 root 角色元数据与直接权限已是当前内建形态时返回 `true`。
// 未删除、系统角色、未停用且直接权限恰为 `root_permission` 时返回 `true`。
//
// `role`：含软删除记录的 root 角色；`permissions`：该角色当前直接权限。
// `root_permission`：期望的 `*:*` 权限。
pub(super) fn root_role_is_current(
    role: &Role,
    permissions: &[Permission],
    root_permission: &Permission,
) -> bool {
    !role.base.is_deleted()
        && role.system
        && !role.disabled
        && permissions == std::slice::from_ref(root_permission)
}

// 冻结 Enforcer 与事务快照 revision 必须完全一致。
// 版本一致时无返回值。
//
// 版本不一致时返回 `Rbac`。
//
// `expected`：冻结 Enforcer 的 policy 版本；`visible`：调用方事务可见的版本。
pub(super) fn ensure_policy_snapshot_revision(expected: u64, visible: u64) -> Result<()> {
    if expected == visible {
        return Ok(());
    }
    Err(Error::Rbac("授权策略版本已变化，无法在当前事务中证明授权快照".to_string()))
}
