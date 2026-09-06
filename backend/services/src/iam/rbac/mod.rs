//! Casbin RBAC 服务。

mod seed;

mod authorize;
mod command;
mod enforcer;
mod policy;
mod query;
mod root;
mod transaction;

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Arc,
    },
};

use casbin::Enforcer;
use database::MongoCasbinAdapter;
use entities::{AccountKind, Permission, PermissionSet};
use mongodb::Database;
use tokio::sync::{Mutex, OnceCell, RwLock};

#[cfg(test)]
use authorize::{
    ensure_all_roles_assignable, ensure_management_subset, ensure_permission_subset, ensure_role_deletable,
    ensure_role_mutable, ensure_roles_delegable, ensure_target_roles_manageable,
};
#[cfg(test)]
use policy::{
    collect_role_ids, collect_role_permissions, commit_outcome_unknown, ensure_policy_snapshot_revision,
    parse_policy_permissions, permission_pairs, permissions_for_roles, policy_revisions_match, role_key,
    role_or_not_found, root_role_is_current, stable_policy_revision,
};

pub use root::ensure_root_role;

pub(crate) const ROOT_ROLE_ID: &str = "role-root";
const ROOT_ROLE_NAME: &str = "超级管理员";
const ROLE_PREFIX: &str = "role:";
const ROOT_ROLE_INIT_ATTEMPTS: usize = 3;
const MAX_STABLE_POLICY_LOAD_ATTEMPTS: usize = 3;

const RBAC_MODEL: &str = r#"
[request_definition]
r = sub, obj, act

[policy_definition]
p = sub, obj, act

[role_definition]
g = _, _

[policy_effect]
e = some(where (p.eft == allow))

[matchers]
m = g(r.sub, p.sub) && (p.obj == "*" || r.obj == p.obj) && (p.act == "*" || r.act == p.act)
"#;

/// 共享 Casbin RBAC 服务。
pub type SharedRbacService = Arc<RbacService>;

/// 以 Casbin 为最终判定源的 RBAC 服务。
pub struct RbacService {
    db: Database,
    policy_store: MongoCasbinAdapter,
    enforcer: OnceCell<RwLock<Enforcer>>,
    loaded_policy_revision: AtomicU64,
    policy_stale: AtomicBool,
    policy_consistency_unknown: AtomicBool,
    policy_write: Arc<Mutex<()>>,
}

/// 一次 Enforcer 读锁下冻结的账号角色与逐角色权限事实。
#[derive(Debug, Clone)]
pub(crate) struct RolePermissionSnapshot {
    role_ids: Vec<String>,
    grants: HashMap<String, HashSet<Permission>>,
    policy_revision: u64,
}

impl RolePermissionSnapshot {
    /// 返回账号在该 policy revision 下直接绑定的角色。
    pub(crate) fn role_ids(&self) -> &[String] {
        &self.role_ids
    }

    /// 返回实际授予指定权限的角色，保持冻结角色顺序。
    pub(crate) fn granting_role_ids(&self, permission: &Permission) -> Vec<String> {
        self.role_ids
            .iter()
            .filter(|role_id| {
                self.grants
                    .get(*role_id)
                    .is_some_and(|permissions| permissions.contains(permission))
            })
            .cloned()
            .collect()
    }

    /// 返回同时授予全部指定权限的角色，保持冻结角色顺序。
    ///
    /// 权限交集必须在同一角色内完成；调用方不得分别汇总不同角色的授权结果。
    pub(crate) fn granting_role_ids_for_all(&self, permissions: &[Permission]) -> Vec<String> {
        if permissions.is_empty() {
            return Vec::new();
        }
        self.role_ids
            .iter()
            .filter(|role_id| {
                self.grants
                    .get(*role_id)
                    .is_some_and(|grants| permissions.iter().all(|permission| grants.contains(permission)))
            })
            .cloned()
            .collect()
    }

    /// 返回冻结 Enforcer 对应的 policy revision。
    pub(crate) fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
}

/// 已基于操作人当前权限校验的角色授予上下文。
pub(crate) struct AuthorizedRoleGrant {
    role_ids: Vec<String>,
    policy_revision: u64,
}

impl AuthorizedRoleGrant {
    /// 返回授权检查使用的 policy 版本。
    pub(crate) fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
}

/// 已校验操作人管理目标账号范围的授权上下文。
pub(crate) struct AuthorizedAccountManagement {
    policy_revision: u64,
    role_grant: Option<AuthorizedRoleGrant>,
}

impl AuthorizedAccountManagement {
    /// 返回授权检查使用的 policy 版本。
    pub(crate) fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    /// 取出同一授权快照校验的可选角色授予上下文。
    pub(crate) fn into_role_grant(self) -> Option<AuthorizedRoleGrant> {
        self.role_grant
    }
}

/// 已基于操作人当前权限校验的角色权限集合。
struct AuthorizedPermissions {
    permissions: PermissionSet,
    policy_revision: u64,
}

/// 已校验当前角色管理范围和可选新权限的上下文。
struct AuthorizedRoleUpdate {
    permissions: Option<PermissionSet>,
    policy_revision: u64,
}

impl RbacService {
    /// 创建 RBAC 服务。
    ///
    /// Casbin Enforcer 在首次使用时异步加载 MongoDB policy。
    pub(crate) fn new(db: Database) -> Self {
        let policy_store = MongoCasbinAdapter::new(db.clone());
        Self {
            db,
            policy_store,
            enforcer: OnceCell::new(),
            loaded_policy_revision: AtomicU64::new(0),
            policy_stale: AtomicBool::new(false),
            policy_consistency_unknown: AtomicBool::new(false),
            policy_write: Arc::new(Mutex::new(())),
        }
    }
}

/// 创建共享 RBAC 服务。
///
/// # 返回值
/// 返回延迟初始化 Casbin Enforcer 的共享服务。
pub fn shared_rbac_service(db: Database) -> SharedRbacService {
    Arc::new(RbacService::new(db))
}

/// 构建 Casbin 主体标识。
///
/// # 返回值
/// 返回包含账号类型和账号 ID 的稳定主体标识。
pub fn subject(account_kind: AccountKind, account_id: &str) -> String {
    format!("user:{}:{account_id}", account_kind.as_str())
}
