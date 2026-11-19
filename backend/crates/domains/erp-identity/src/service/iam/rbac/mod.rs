//! Casbin RBAC 服务。

mod database_reset;
mod seed_command;
mod templates;

mod authorize;
mod bundle;
mod command;
mod enforcer;
mod policy;
mod qualification;
mod query;
pub use qualification::QualifiedPermissions;
mod root;
mod transaction;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

#[cfg(test)]
use authorize::{
    ensure_all_roles_assignable, ensure_management_subset, ensure_permission_subset, ensure_roles_delegable,
    ensure_target_roles_manageable,
};
use casbin::Enforcer;
use erp_core::AccountKind;
use mongodb::Database;
#[cfg(test)]
use policy::{
    collect_role_ids, collect_role_permissions, commit_outcome_unknown, ensure_policy_snapshot_revision,
    parse_policy_permissions, permission_pairs, permissions_for_roles, role_key, role_or_not_found,
    root_role_is_current,
};
pub use root::ensure_root_role;
use tokio::sync::{Mutex, OnceCell, RwLock};

use crate::MongoCasbinAdapter;
use crate::entity::rbac::{Permission, PermissionSet, RoleId, RoleIdSet};
pub use crate::entity::role::ROOT_ROLE_ID;
use crate::ports::IdentityAuditPort;
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

/// 以 Casbin 策略为角色绑定和功能权限的权威来源，供身份管理与统一授权服务使用。
pub struct RbacService {
    // 供身份仓储访问账号和角色等元数据的数据库句柄。
    db: Database,
    // 从 MongoDB 加载和写入 Casbin 规则、绑定及全局策略版本的适配器。
    policy_store: MongoCasbinAdapter,
    // 准备和持久化角色、绑定等管理操作的审计。
    audit: Arc<dyn IdentityAuditPort>,
    // 首次使用时异步初始化的本地 Enforcer，以读写锁隔离判定与刷新。
    enforcer: OnceCell<RwLock<Enforcer>>,
    // 本地 Enforcer 最近一次稳定加载的策略版本，供缓存对齐和授权取证。
    loaded_policy_revision: AtomicU64,
    // 策略缓存须重载的标记，成功取得稳定快照后才清除。
    policy_stale: AtomicBool,
    // 提交结果未知时阻断本进程的鉴权和后续策略事务，须重启才能解除。
    policy_consistency_unknown: AtomicBool,
    // 串行化同一服务实例的策略事务及提交后的刷新。
    policy_write: Arc<Mutex<()>>,
}

/// 同一 Enforcer 读锁下冻结的账号角色与逐角色权限事实，供后续资格判断使用。
#[derive(Debug, Clone)]
pub struct RolePermissionSnapshot {
    // 冻结时直接绑定的角色 ID；资格判断方另行核验启用状态。
    role_ids: Vec<RoleId>,
    // 每个角色对本次指定权限的实际命中，用于保留同角色授予语义。
    grants: HashMap<RoleId, HashSet<Permission>>,
    // 冻结时的 Enforcer 策略版本，供调用方与原执行器所见版本比对。
    policy_revision: u64,
}

impl RolePermissionSnapshot {
    /// 返回冻结的直接绑定角色，供调用方核对角色状态和后续资格判断。
    pub fn role_ids(&self) -> &[RoleId] {
        &self.role_ids
    }

    /// 按冻结角色顺序返回授予指定权限的角色 ID，供判定单项权限及其授予来源。
    pub fn granting_role_ids(&self, permission: &Permission) -> Vec<RoleId> {
        self.role_ids
            .iter()
            .filter(|role_id| {
                self.grants.get(*role_id).is_some_and(|permissions| permissions.contains(permission))
            })
            .cloned()
            .collect()
    }

    /// 校验同角色资格时，在每个角色内核对全部要求，按冻结顺序返回能独立授予整组权限的角色 ID。
    /// 禁止合并不同角色的授权结果；`permissions` 为空时返回空列表。
    pub fn granting_role_ids_for_all(&self, permissions: &[Permission]) -> Vec<RoleId> {
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

    /// 仅在授权已拒绝时，用完整功能权限要求区分单项缺权与同一有效角色配置不满足。
    /// 单项缺权沿用稳定文案，供 HTTP 边界提取权限字段；该诊断仅反映功能资格。
    pub fn denial_message(&self, permissions: &[Permission]) -> String {
        permissions.iter().find(|permission| self.granting_role_ids(permission).is_empty()).map_or_else(
            || "所需操作权限未由同一有效角色授予，请联系管理员检查角色配置".into(),
            |permission| format!("当前账号缺少 {permission} 权限"),
        )
    }

    /// 返回冻结 Enforcer 对应的策略版本，供资格或策略事务复核。
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
}

/// 已按操作人当前权限校验的角色授予上下文，供普通管理入口替换账号绑定。
pub struct AuthorizedRoleGrant {
    // 通过授予检查的完整角色集合，写入时整体替换原绑定。
    role_ids: RoleIdSet,
    // 授予检查所用的策略版本，绑定事务须匹配此版本。
    policy_revision: u64,
}

impl AuthorizedRoleGrant {
    /// 开启角色绑定事务前，读取本次授予检查所依据的策略版本。
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
}

/// 已校验操作人能管理目标账号的上下文，供后续账号修改或删除事务使用。
pub struct AuthorizedAccountManagement {
    // 目标账号管理检查所用的策略版本，供写事务重验。
    policy_revision: u64,
    // 请求更换角色时在同一策略快照校验的授予上下文；空值表示保留绑定。
    role_grant: Option<AuthorizedRoleGrant>,
}

impl AuthorizedAccountManagement {
    /// 开启账号管理事务前，读取本次管理检查所依据的策略版本。
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    /// 准备账号写入时消费管理上下文，取出同一快照校验的授予上下文；未请求更换角色时返回 `None`。
    pub fn into_role_grant(self) -> Option<AuthorizedRoleGrant> {
        self.role_grant
    }
}

// 已按操作人授予上限校验的角色权限，供角色创建或种子命令使用。
struct AuthorizedPermissions {
    // 通过子集校验且已去重的拟授予权限。
    permissions: PermissionSet,
    // 校验时的策略版本，供写事务比对。
    policy_revision: u64,
}

// 已校验角色现有权限可管理及拟授予权限未越界，供编辑或删除角色使用。
struct AuthorizedRoleUpdate {
    // 经校验的整套新直接权限；空值表示不替换权限。
    permissions: Option<PermissionSet>,
    // 管理和授予检查共用的策略版本，供写事务比对。
    policy_revision: u64,
}

impl RbacService {
    /// 组装 RBAC 服务的数据库和审计依赖；Casbin Enforcer 在首次使用时才异步加载 MongoDB 策略。
    pub fn new(db: Database, audit: Arc<dyn IdentityAuditPort>) -> Self {
        let policy_store = MongoCasbinAdapter::new(db.clone());
        Self {
            db,
            policy_store,
            audit,
            enforcer: OnceCell::new(),
            loaded_policy_revision: AtomicU64::new(0),
            policy_stale: AtomicBool::new(false),
            policy_consistency_unknown: AtomicBool::new(false),
            policy_write: Arc::new(Mutex::new(())),
        }
    }

    /// 提供身份数据库句柄，供启动期组织引导复用同一连接。
    pub(crate) fn database(&self) -> &Database {
        &self.db
    }

    /// 管理写入前通过审计端口准备成功资源审计，供业务事务一并持久化；构造失败时返回原错误。
    pub fn prepare_resource_log(
        &self,
        actor: application_core::AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> crate::error::Result<crate::ports::PreparedResourceAudit> {
        self.audit.resource_log(actor, action, resource_type, resource_id)
    }
}

/// 在应用装配时创建共享的 RBAC 服务，延迟到首次使用才加载 Casbin Enforcer。
pub fn shared_rbac_service(db: Database, audit: Arc<dyn IdentityAuditPort>) -> SharedRbacService {
    Arc::new(RbacService::new(db, audit))
}

/// 读写 Casbin 绑定时，将账号类型与 ID 编为稳定的 `user:{kind}:{account_id}` 主体，区分不同类型的账号。
pub fn subject(account_kind: AccountKind, account_id: &str) -> String {
    format!("user:{}:{account_id}", account_kind.as_str())
}
