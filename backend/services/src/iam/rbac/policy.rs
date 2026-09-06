use std::collections::HashMap;

use casbin::{Enforcer, RbacApi};
use entities::{AccountKind, Permission, PermissionSet, Role};

use super::{subject, ROLE_PREFIX};
use crate::{
    audit::AuditActor,
    errors::{Error, Result},
};

pub(super) fn role_key(role_id: &str) -> String {
    format!("{ROLE_PREFIX}{role_id}")
}

pub(super) fn permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<Vec<Permission>> {
    parse_policy_permissions(enforcer.get_permissions_for_user(&role_key(role_id), None))
}

pub(super) fn implicit_permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&role_key(role_id), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

pub(super) fn collect_role_permissions(
    enforcer: &Enforcer,
    role_ids: &[String],
) -> Result<HashMap<String, Vec<Permission>>> {
    role_ids
        .iter()
        .map(|role_id| Ok((role_id.clone(), permissions_for_role(enforcer, role_id)?)))
        .collect()
}

pub(super) fn permissions_for_actor(enforcer: &Enforcer, actor: &AuditActor) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(actor.kind(), actor.id()), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

pub(super) fn permissions_for_account(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_id: &str,
) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

pub(super) fn permissions_for_roles(enforcer: &Enforcer, role_ids: &[String]) -> Result<PermissionSet> {
    let mut permissions = Vec::new();
    for role_id in role_ids {
        permissions.extend(implicit_permissions_for_role(enforcer, role_id)?.into_vec());
    }
    Ok(PermissionSet::new(permissions))
}

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

pub(super) fn collect_role_ids(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_ids: &[String],
) -> HashMap<String, Vec<String>> {
    account_ids
        .iter()
        .map(|account_id| {
            (
                account_id.clone(),
                role_ids_for_account(enforcer, account_kind, account_id),
            )
        })
        .collect()
}

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

pub(super) fn permission_pairs(permissions: Vec<Permission>) -> Vec<(String, String)> {
    PermissionSet::new(permissions)
        .into_vec()
        .into_iter()
        .map(|permission| (permission.resource().to_string(), permission.action().to_string()))
        .collect()
}

pub(super) fn role_or_not_found(role: Option<Role>) -> Result<Role> {
    role.ok_or_else(|| Error::NotFound("角色不存在".to_string()))
}

pub(super) fn commit_outcome_unknown(error: &Error) -> bool {
    matches!(error, Error::OutcomeUnknown(_))
}

pub(super) fn stable_policy_revision(before: u64, after: u64) -> Option<u64> {
    (before == after).then_some(after)
}

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

pub(super) fn policy_revisions_match(loaded: u64, database: u64) -> bool {
    loaded == database
}

/// 冻结 Enforcer 与事务快照 revision 必须完全一致。
pub(super) fn ensure_policy_snapshot_revision(expected: u64, visible: u64) -> Result<()> {
    if expected == visible {
        return Ok(());
    }
    Err(Error::Rbac(
        "授权策略版本已变化，无法在当前事务中证明授权快照".to_string(),
    ))
}

pub(super) fn rbac_error(error: impl std::fmt::Display) -> Error {
    Error::Rbac(error.to_string())
}
