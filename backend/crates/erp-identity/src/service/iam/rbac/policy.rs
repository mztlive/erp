use std::collections::HashMap;

use application_core::AuditActor;
use casbin::{Enforcer, RbacApi};
use erp_core::AccountKind;

use super::{ROLE_PREFIX, subject};
use crate::entity::{Permission, PermissionSet, Role};
use crate::error::{Error, Result};

/// 把角色 ID 拼成 Casbin 角色主体。
///
/// # 参数
/// * `role_id` - 不含 `role:` 前缀的角色 ID。
///
/// # 返回
/// 返回 `role:` 前缀加角色 ID。
///
/// # 错误
/// 不返回错误。
pub(super) fn role_key(role_id: &str) -> String {
    format!("{ROLE_PREFIX}{role_id}")
}

/// 读取角色直接权限，不展开继承。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `role_id` - 角色 ID。
///
/// # 返回
/// 返回去重后的直接权限。
///
/// # 错误
/// 策略行无法解析为权限时返回错误。
pub(super) fn permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<Vec<Permission>> {
    parse_policy_permissions(enforcer.get_permissions_for_user(&role_key(role_id), None))
}

/// 读取角色的隐式权限并收成权限集。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `role_id` - 角色 ID。
///
/// # 返回
/// 返回包含继承权限的 `PermissionSet`。
///
/// # 错误
/// 策略行无法解析为权限时返回错误。
pub(super) fn implicit_permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&role_key(role_id), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

/// 按角色收集直接权限。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `role_ids` - 角色 ID 列表。
///
/// # 返回
/// 返回角色 ID 到直接权限的映射。
///
/// # 错误
/// 任一角色的策略无法解析时返回错误。
pub(super) fn collect_role_permissions(
    enforcer: &Enforcer,
    role_ids: &[String],
) -> Result<HashMap<String, Vec<Permission>>> {
    role_ids.iter().map(|role_id| Ok((role_id.clone(), permissions_for_role(enforcer, role_id)?))).collect()
}

/// 读取操作人经角色继承得到的权限集。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `actor` - 已认证操作人。
///
/// # 返回
/// 返回操作人的隐式权限集。
///
/// # 错误
/// 策略行无法解析为权限时返回错误。
pub(super) fn permissions_for_actor(enforcer: &Enforcer, actor: &AuditActor) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(actor.kind(), actor.id()), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

/// 读取账号经角色继承得到的权限集。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `account_kind` - 账号类型。
/// * `account_id` - 账号 ID。
///
/// # 返回
/// 返回该账号的隐式权限集。
///
/// # 错误
/// 策略行无法解析为权限时返回错误。
pub(super) fn permissions_for_account(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_id: &str,
) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

/// 合并多个角色的隐式权限。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `role_ids` - 角色 ID 列表。
///
/// # 返回
/// 返回这些角色隐式权限的并集。
///
/// # 错误
/// 任一角色的策略无法解析时返回错误。
pub(super) fn permissions_for_roles(enforcer: &Enforcer, role_ids: &[String]) -> Result<PermissionSet> {
    let mut permissions = Vec::new();
    for role_id in role_ids {
        permissions.extend(implicit_permissions_for_role(enforcer, role_id)?.into_vec());
    }
    Ok(PermissionSet::new(permissions))
}

/// 读取账号直接绑定的角色 ID。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `account_kind` - 账号类型。
/// * `account_id` - 账号 ID。
///
/// # 返回
/// 返回去掉 `role:` 前缀的角色 ID；非角色主体被忽略。
///
/// # 错误
/// 不返回错误。
pub(super) fn role_ids_for_account(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_id: &str,
) -> Vec<String> {
    enforcer
        .get_roles_for_user(&subject(account_kind, account_id), None)
        .into_iter()
        .filter_map(|role| role.strip_prefix(ROLE_PREFIX).map(str::to_string))
        .collect()
}

/// 批量读取同类账号的直接角色绑定。
///
/// # 参数
/// * `enforcer` - 已加载的 Casbin Enforcer。
/// * `account_kind` - 账号类型。
/// * `account_ids` - 账号 ID 列表。
///
/// # 返回
/// 返回账号 ID 到角色 ID 列表的映射；未绑定的账号对应空列表。
///
/// # 错误
/// 不返回错误。
pub(super) fn collect_role_ids(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_ids: &[String],
) -> HashMap<String, Vec<String>> {
    account_ids
        .iter()
        .map(|account_id| (account_id.clone(), role_ids_for_account(enforcer, account_kind, account_id)))
        .collect()
}

/// 把 Casbin 策略行解析为去重后的权限。
///
/// # 参数
/// * `policies` - Casbin 策略行，列至少包含主体、资源、动作。
///
/// # 返回
/// 返回可解析行组成的权限列表；列数不足的行被跳过。
///
/// # 错误
/// 资源或动作无法组成合法权限时返回错误。
pub(super) fn parse_policy_permissions(policies: Vec<Vec<String>>) -> Result<Vec<Permission>> {
    let mut permissions = Vec::new();
    for policy in policies {
        let [_, resource, action] = policy.as_slice() else {
            continue;
        };
        permissions.push(Permission::parse(format!("{resource}:{action}"))?);
    }
    Ok(PermissionSet::new(permissions).into_vec())
}

/// 把权限展开为 Casbin 的资源、动作对。
///
/// # 参数
/// * `permissions` - 待写入的权限。
///
/// # 返回
/// 返回去重后的 `(resource, action)` 列表。
///
/// # 错误
/// 不返回错误。
pub(super) fn permission_pairs(permissions: Vec<Permission>) -> Vec<(String, String)> {
    PermissionSet::new(permissions)
        .into_vec()
        .into_iter()
        .map(|permission| (permission.resource().to_string(), permission.action().to_string()))
        .collect()
}

/// 把缺失角色收成统一的不存在错误。
///
/// # 参数
/// * `role` - 仓储读到的可选角色。
///
/// # 返回
/// 角色存在时返回该角色。
///
/// # 错误
/// `role` 为 `None` 时返回 `NotFound`。
pub(super) fn role_or_not_found(role: Option<Role>) -> Result<Role> {
    role.ok_or_else(|| Error::NotFound("角色不存在".to_string()))
}

/// 判断错误链里是否有提交结果未知。
///
/// # 参数
/// * `error` - 待检查的错误。
///
/// # 返回
/// 自身或 `source` 链上出现 `CommitOutcomeUnknown` 或 `OutcomeUnknown` 时返回 `true`。
///
/// # 错误
/// 不返回错误。
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

/// 仅当加载前后的 policy 版本相同才视为稳定快照。
///
/// # 参数
/// * `before` - 加载前的 policy 版本。
/// * `after` - 加载后的 policy 版本。
///
/// # 返回
/// 两次版本相同返回 `Some(after)`，否则返回 `None`。
///
/// # 错误
/// 不返回错误。
pub(super) fn stable_policy_revision(before: u64, after: u64) -> Option<u64> {
    (before == after).then_some(after)
}

/// 判断 root 角色元数据与直接权限是否已是当前内建形态。
///
/// # 参数
/// * `role` - 含软删除记录的 root 角色。
/// * `permissions` - 该角色当前直接权限。
/// * `root_permission` - 期望的 `*:*` 权限。
///
/// # 返回
/// 未删除、系统角色、未停用且直接权限恰为 `root_permission` 时返回 `true`。
///
/// # 错误
/// 不返回错误。
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

/// 比较已加载版本与数据库版本。
///
/// # 参数
/// * `loaded` - 本地 Enforcer 已加载的版本。
/// * `database` - 数据库当前版本。
///
/// # 返回
/// 两者相等时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub(super) fn policy_revisions_match(loaded: u64, database: u64) -> bool {
    loaded == database
}

/// 冻结 Enforcer 与事务快照 revision 必须完全一致。
///
/// # 参数
/// * `expected` - 冻结 Enforcer 的 policy 版本。
/// * `visible` - 调用方事务可见的版本。
///
/// # 返回
/// 版本一致时无返回值。
///
/// # 错误
/// 版本不一致时返回 `Rbac`。
pub(super) fn ensure_policy_snapshot_revision(expected: u64, visible: u64) -> Result<()> {
    if expected == visible {
        return Ok(());
    }
    Err(Error::Rbac("授权策略版本已变化，无法在当前事务中证明授权快照".to_string()))
}

/// 把展示用错误收成 RBAC 错误。
///
/// # 参数
/// * `error` - 可展示的下层错误。
///
/// # 返回
/// 返回 `Error::Rbac`。
///
/// # 错误
/// 不返回错误。
pub(super) fn rbac_error(error: impl std::fmt::Display) -> Error {
    Error::Rbac(error.to_string())
}
