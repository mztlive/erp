use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::AccountKind;

use super::{ROLE_PREFIX, subject};
use crate::entity::{Permission, PermissionSet, Role, RoleId, RoleIdSet};
use crate::error::{Error, Result};
use crate::repository::policy_store::PolicySnapshot;

/// 读写策略规则时，为未带前缀的角色 ID 加上 `role:`，与账号主体区分。
pub(super) fn role_key(role_id: &str) -> String {
    format!("{ROLE_PREFIX}{role_id}")
}

/// 展示角色配置或核对种子、内建角色时，从已加载的策略快照读取去重后的直接权限；继承权限不计入。
pub(super) fn permissions_for_role(snapshot: &PolicySnapshot, role_id: &str) -> Result<PermissionSet> {
    snapshot.direct_permissions(&role_key(role_id))
}

/// 计算角色的授予或管理上限时，从已加载的策略快照读取包含继承在内的隐式权限集。
pub(super) fn implicit_permissions_for_role(
    snapshot: &PolicySnapshot,
    role_id: &str,
) -> Result<PermissionSet> {
    snapshot.implicit_permissions(&role_key(role_id))
}

/// 管理写入预检时，按操作人的身份从已加载的策略快照读取经角色继承的隐式权限上限。
pub(super) fn permissions_for_actor(snapshot: &PolicySnapshot, actor: &AuditActor) -> Result<PermissionSet> {
    snapshot.implicit_permissions(&subject(actor.kind(), actor.id()))
}

/// 管理目标账号前，按账号类型与 ID 从已加载的策略快照读取经角色继承的当前权限。
pub(super) fn permissions_for_account(
    snapshot: &PolicySnapshot,
    account_kind: AccountKind,
    account_id: &str,
) -> Result<PermissionSet> {
    snapshot.implicit_permissions(&subject(account_kind, account_id))
}

/// 校验拟分配的角色集合时，从同一策略快照合并各角色隐式权限，供授予上限检查。
pub(super) fn permissions_for_roles(
    snapshot: &PolicySnapshot,
    role_ids: &[impl AsRef<str>],
) -> Result<PermissionSet> {
    let mut permissions = Vec::new();
    for role_id in role_ids {
        permissions.extend(implicit_permissions_for_role(snapshot, role_id.as_ref())?.into_vec());
    }
    Ok(PermissionSet::new(permissions))
}

/// 查询绑定或冻结资格快照时，读取账号直接绑定的角色 ID；跳过非角色主体，移除 `role:` 前缀。
pub(super) fn role_ids_for_account(
    snapshot: &PolicySnapshot,
    account_kind: AccountKind,
    account_id: &str,
) -> Result<Vec<RoleId>> {
    Ok(RoleIdSet::from_casbin_role_keys(snapshot.direct_roles(&subject(account_kind, account_id)))?.into())
}

/// 批量查询同类账号时，从同一策略快照按账号收集直接角色 ID；无绑定的账号对应空列表。
pub(super) fn collect_role_ids(
    snapshot: &PolicySnapshot,
    account_kind: AccountKind,
    account_ids: &[String],
) -> Result<HashMap<String, Vec<RoleId>>> {
    account_ids
        .iter()
        .map(|account_id| Ok((account_id.clone(), role_ids_for_account(snapshot, account_kind, account_id)?)))
        .collect()
}

/// 写入角色策略前，将权限规范化并转换为去重的 `(resource, action)` 对，角色主体由写入方另行指定。
pub(super) fn permission_pairs(permissions: Vec<Permission>) -> Vec<(String, String)> {
    PermissionSet::new(permissions)
        .into_vec()
        .into_iter()
        .map(|permission| (permission.resource().to_string(), permission.action().to_string()))
        .collect()
}

/// 管理命令读取必需角色后，提取仓储结果；角色缺失时统一返回 `NotFound`。
pub(super) fn role_or_not_found(role: Option<Role>) -> Result<Role> {
    role.ok_or_else(|| Error::NotFound("角色不存在".to_string()))
}

/// 策略事务失败后检查错误及 `source` 链；遇到 `CommitOutcomeUnknown` 或 `OutcomeUnknown` 时标识提交结果未知。
pub(super) fn commit_outcome_unknown(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(err) = current {
        if matches!(
            err.downcast_ref::<persistence_core::Error>(),
            Some(persistence_core::Error::CommitOutcomeUnknown(_))
        ) || matches!(err.downcast_ref::<Error>(), Some(Error::OutcomeUnknown(_)))
        {
            return true;
        }
        current = err.source();
    }
    false
}

/// 初始化时判断内建 `role-root` 是否已就绪：角色未删除、已启用且为系统角色，直接权限恰为预期的 `*:*`。
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

/// 使用策略快照前，比较冻结的策略版本与事务可见版本；两者不完全一致时返回 `Rbac`。
pub(super) fn ensure_policy_snapshot_revision(expected: u64, visible: u64) -> Result<()> {
    if expected == visible {
        return Ok(());
    }
    Err(Error::Rbac("授权策略版本已变化，无法在当前事务中证明授权快照".to_string()))
}
