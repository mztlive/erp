//! Casbin RBAC 服务。

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use casbin::{CoreApi, DefaultModel, Enforcer, RbacApi};
use database::{DatabaseExt, MongoCasbinAdapter, Transactional};
use entities::{AccountKind, AuditLog, Permission, PermissionSet, Role, RoleData, RoleIdSet, RoleUpdate};
use mongodb::{ClientSession, Database};
use tokio::sync::{Mutex, OnceCell, RwLock};

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    iam::{CreateRoleParams, RoleItem, UpdateRoleParams},
    owned_task::await_owned,
};

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

    async fn enforcer(&self) -> Result<&RwLock<Enforcer>> {
        self.enforcer
            .get_or_try_init(|| async {
                let model = DefaultModel::from_str(RBAC_MODEL).await.map_err(rbac_error)?;
                let adapter = self.policy_store.clone();
                let mut enforcer = Enforcer::new_raw(model, adapter).await.map_err(rbac_error)?;
                enforcer.enable_auto_save(false);
                self.reload_enforcer(&mut enforcer).await?;
                Ok(RwLock::new(enforcer))
            })
            .await
    }

    /// 返回已恢复到最新 MongoDB policy 的 Enforcer。
    ///
    /// 上一次持久化或 reload 失败后，任何鉴权和 policy 查询都会先重试 reload；
    /// reload 继续失败时直接返回错误，禁止使用旧授权缓存。
    async fn fresh_enforcer(&self) -> Result<&RwLock<Enforcer>> {
        self.ensure_policy_consistency_known()?;
        let enforcer = self.enforcer().await?;
        if self.policy_cache_is_current().await? {
            self.ensure_policy_consistency_known()?;
            return Ok(enforcer);
        }

        let mut guard = enforcer.write().await;
        if !self.policy_cache_is_current().await? {
            self.reload_enforcer(&mut guard).await?;
        }
        drop(guard);
        self.ensure_policy_consistency_known()?;
        Ok(enforcer)
    }

    /// 判断本地 Enforcer 是否对应数据库当前 policy 版本。
    async fn policy_cache_is_current(&self) -> Result<bool> {
        if self.policy_stale.load(Ordering::Acquire) {
            return Ok(false);
        }
        let database_revision = self.policy_store.policy_revision().await?;
        Ok(policy_revisions_match(
            self.loaded_policy_revision.load(Ordering::Acquire),
            database_revision,
        ))
    }

    /// 从 MongoDB 重新加载稳定 policy 快照，并维护 fail-closed stale 状态。
    async fn reload_enforcer(&self, enforcer: &mut Enforcer) -> Result<()> {
        self.policy_stale.store(true, Ordering::Release);
        for _ in 0..MAX_STABLE_POLICY_LOAD_ATTEMPTS {
            let before = self.policy_store.policy_revision().await?;
            enforcer.load_policy().await.map_err(rbac_error)?;
            let after = self.policy_store.policy_revision().await?;
            if let Some(revision) = stable_policy_revision(before, after) {
                self.loaded_policy_revision.store(revision, Ordering::Release);
                self.policy_stale.store(false, Ordering::Release);
                return Ok(());
            }
        }
        Err(Error::Rbac(
            "授权策略持续变化，无法加载稳定快照，请稍后重试".to_string(),
        ))
    }

    /// 提交外部 policy 事务后，立即刷新本地 Enforcer。
    ///
    /// reload 失败时服务保持 stale；后续鉴权和 policy 查询会先重试，
    /// 在恢复前不会继续使用旧缓存。
    ///
    /// # 错误
    /// 当 MongoDB policy 无法重新加载时返回错误。
    async fn refresh_policy(&self) -> Result<()> {
        self.fresh_enforcer().await.map(|_| ())
    }

    /// 判断账号是否具有指定权限。
    ///
    /// # 错误
    /// 当 Casbin policy 加载或匹配失败时返回错误。
    pub async fn enforce(&self, subject: &str, permission: &Permission) -> Result<bool> {
        self.fresh_enforcer()
            .await?
            .read()
            .await
            .enforce((subject, permission.resource(), permission.action()))
            .map_err(rbac_error)
    }

    /// 创建角色并写入 Casbin 权限策略。
    ///
    /// # 参数
    /// * `params` - 创建角色参数
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回值
    /// 返回创建后的角色实体。
    ///
    /// # 错误
    /// 当角色校验、持久化或 Casbin policy 写入失败时返回错误。
    pub async fn create_role(self: &Arc<Self>, params: CreateRoleParams, actor: AuditActor) -> Result<Role> {
        let authorized = self.authorize_permissions(&actor, params.permissions).await?;
        let id = id_generator::next_id();
        let audit = actor.resource_log("role.create", "role", id.clone())?;
        self.create_role_with_id(
            id,
            RoleData {
                name: params.name,
                description: None,
                system: false,
            },
            authorized.permissions.into_vec(),
            Some(audit),
            Some(authorized.policy_revision),
        )
        .await
    }

    /// 更新角色信息并按需覆盖权限。
    ///
    /// # 参数
    /// * `id` - 待更新角色 ID
    /// * `params` - 可选名称与权限更新
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回值
    /// 返回更新后的角色实体。
    ///
    /// # 错误
    /// 当角色不存在、校验失败或持久化失败时返回错误。
    pub async fn update_role(
        self: &Arc<Self>,
        id: &str,
        params: UpdateRoleParams,
        actor: AuditActor,
    ) -> Result<Role> {
        let role = role_or_not_found(self.db.roles().find_by_id(id).await?)?;
        ensure_role_mutable(&role)?;
        let authorized = self
            .authorize_role_update(&actor, role.base.id.as_str(), params.permissions)
            .await?;
        let audit = actor.resource_log("role.update", "role", role.base.id.clone())?;
        let AuthorizedRoleUpdate {
            permissions,
            policy_revision,
        } = authorized;

        match (params.name, permissions) {
            (None, None) => self.audit_role_update(role, audit, policy_revision).await,
            (Some(name), None) => self.update_role_name(role, name, audit, policy_revision).await,
            (None, Some(permissions)) => {
                self.replace_role_permissions(
                    role.base.id.as_str(),
                    permissions.into_vec(),
                    Some(audit),
                    Some(policy_revision),
                )
                .await
            }
            (Some(name), Some(permissions)) => {
                self.update_role_with_permissions(role, name, permissions, policy_revision, audit)
                    .await
            }
        }
    }

    /// 删除非系统角色及其 Casbin policy/绑定。
    ///
    /// # 参数
    /// * `id` - 待删除角色 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回值
    /// 角色实体与关联 policy 原子删除成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当角色不存在、角色为系统角色或删除失败时返回错误。
    pub async fn delete_role(self: &Arc<Self>, id: &str, actor: AuditActor) -> Result<()> {
        let role = role_or_not_found(self.db.roles().find_by_id(id).await?)?;
        ensure_role_deletable(&role)?;
        let authorized = self
            .authorize_role_update(&actor, role.base.id.as_str(), None)
            .await?;
        let audit = actor.resource_log("role.delete", "role", role.base.id.clone())?;

        self.delete_role_with_policy(role, audit, authorized.policy_revision)
            .await
    }

    /// 为成功的无字段变更请求写入审计日志。
    async fn audit_role_update(
        self: &Arc<Self>,
        role: Role,
        audit: AuditLog,
        policy_revision: u64,
    ) -> Result<Role> {
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |_| {
            Box::pin(async move { Ok(role) })
        })
        .await
    }

    /// 在同一事务中更新角色名称并写入审计日志。
    async fn update_role_name(
        self: &Arc<Self>,
        mut role: Role,
        name: String,
        audit: AuditLog,
        policy_revision: u64,
    ) -> Result<Role> {
        role.update(RoleUpdate {
            name: Some(name),
            ..Default::default()
        })?;
        let db = self.db.clone();
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
            Box::pin(async move {
                db.roles().update_with_session(&mut role, session).await?;
                Ok(role)
            })
        })
        .await
    }

    /// 查询全部角色及其直接权限。
    ///
    /// # 错误
    /// 当 MongoDB 或 Casbin policy 查询失败时返回错误。
    pub async fn role_list(&self) -> Result<Vec<RoleItem>> {
        let roles = self.db.roles().list_all().await?;
        self.role_items(roles).await
    }

    /// 查询全部可分配角色及其直接权限。
    ///
    /// # 错误
    /// 当 MongoDB 或 Casbin policy 查询失败时返回错误。
    pub async fn assignable_role_list(&self, actor: &AuditActor) -> Result<Vec<RoleItem>> {
        let roles = self
            .db
            .roles()
            .list_enabled()
            .await?
            .into_iter()
            .filter(role_is_assignable)
            .collect::<Vec<_>>();
        self.assignable_role_items(roles, actor).await
    }

    /// 在同一事务中 CAS touch 角色并覆盖权限。
    ///
    /// 角色写触碰会递增版本与更新时间，使权限更新、角色绑定和角色删除在多实例部署下
    /// 对同一角色产生写冲突，避免删除后遗留 policy 引用。
    async fn replace_role_permissions(
        self: &Arc<Self>,
        role_id: &str,
        permissions: Vec<Permission>,
        audit: Option<AuditLog>,
        expected_revision: Option<u64>,
    ) -> Result<Role> {
        let role_id = role_id.to_string();
        let role_key = role_key(&role_id);
        let permissions = permission_pairs(permissions);
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        self.run_policy_transaction_at_revision(expected_revision, move |session| {
            Box::pin(async move {
                let mut role =
                    role_or_not_found(db.roles().find_by_id_with_session(&role_id, session).await?)?;
                db.roles().update_with_session(&mut role, session).await?;
                policy_store
                    .replace_role_permissions_with_session(&role_key, &permissions, session)
                    .await?;
                if let Some(audit) = audit {
                    db.audit_logs().create_with_session(&audit, session).await?;
                }
                Ok(role)
            })
        })
        .await
    }

    /// 按操作人权限范围过滤可分配角色；整个批次只读取一次 Enforcer。
    async fn assignable_role_items(&self, roles: Vec<Role>, actor: &AuditActor) -> Result<Vec<RoleItem>> {
        if roles.is_empty() {
            return Ok(Vec::new());
        }

        let role_ids = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
        let enforcer = self.fresh_enforcer().await?.read().await;
        let actor_permissions = permissions_for_actor(&enforcer, actor)?;
        let mut permissions = collect_role_permissions(&enforcer, &role_ids)?;
        let mut items = Vec::with_capacity(roles.len());
        for role in roles {
            let role_permissions = permissions.remove(&role.base.id).unwrap_or_default();
            let required_permissions = implicit_permissions_for_role(&enforcer, role.base.id.as_str())?;
            if actor_permissions.covers(&required_permissions) {
                items.push(RoleItem::from_role(role, role_permissions));
            }
        }
        Ok(items)
    }

    /// 为一批角色装配直接权限；整个批次只读取一次 Enforcer。
    async fn role_items(&self, roles: Vec<Role>) -> Result<Vec<RoleItem>> {
        if roles.is_empty() {
            return Ok(Vec::new());
        }

        let role_ids = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
        let enforcer = self.fresh_enforcer().await?.read().await;
        let mut permissions = collect_role_permissions(&enforcer, &role_ids)?;

        Ok(roles
            .into_iter()
            .map(|role| {
                let role_permissions = permissions.remove(&role.base.id).unwrap_or_default();
                RoleItem::from_role(role, role_permissions)
            })
            .collect())
    }

    /// 校验操作人是否可以授予目标角色，并捕获当前 policy 版本。
    ///
    /// 系统角色不允许通过普通管理接口分配；目标角色权限必须是操作人当前隐式权限的子集。
    ///
    /// # 错误
    /// 当角色不存在、不可分配、权限越界或 policy 加载失败时返回错误。
    pub(crate) async fn authorize_role_assignment(
        &self,
        actor: &AuditActor,
        role_ids: Vec<String>,
    ) -> Result<AuthorizedRoleGrant> {
        let role_ids = RoleIdSet::parse(role_ids)?.to_strings();
        let roles = self.db.roles().enabled_roles(&role_ids).await?;
        ensure_all_roles_assignable(role_ids.len(), roles.len())?;
        ensure_roles_delegable(&roles)?;

        let enforcer = self.fresh_enforcer().await?.read().await;
        let actor_permissions = permissions_for_actor(&enforcer, actor)?;
        let required_permissions = permissions_for_roles(&enforcer, &role_ids)?;
        ensure_permission_subset(&actor_permissions, &required_permissions)?;
        let policy_revision = self.loaded_policy_revision.load(Ordering::Acquire);
        Ok(AuthorizedRoleGrant {
            role_ids,
            policy_revision,
        })
    }

    /// 校验操作人可管理目标账号，并按需校验新的角色集合。
    ///
    /// 目标账号当前权限与待分配角色权限都必须是操作人权限的子集；绑定任一系统
    /// 角色的账号禁止通过普通管理接口修改。整个判断共享同一 Enforcer 快照。
    ///
    /// # 错误
    /// 当目标含系统角色、权限越界、待分配角色无效或 policy 加载失败时返回错误。
    pub(crate) async fn authorize_target_management(
        &self,
        actor: &AuditActor,
        target_kind: AccountKind,
        target_id: &str,
        requested_role_ids: Option<Vec<String>>,
    ) -> Result<AuthorizedAccountManagement> {
        let requested_role_ids = requested_role_ids
            .map(RoleIdSet::parse)
            .transpose()?
            .map(|role_ids| role_ids.to_strings());
        let enforcer = self.fresh_enforcer().await?.read().await;
        let actor_permissions = permissions_for_actor(&enforcer, actor)?;
        let target_role_ids = role_ids_for_account(&enforcer, target_kind, target_id);
        authorize_account_permissions(
            &enforcer,
            &actor_permissions,
            target_kind,
            target_id,
            requested_role_ids.as_deref(),
        )?;
        let policy_revision = self.loaded_policy_revision.load(Ordering::Acquire);
        drop(enforcer);

        let roles = self
            .load_management_roles(&target_role_ids, requested_role_ids.as_deref())
            .await?;
        ensure_target_roles_manageable(&target_role_ids, &roles)?;
        let role_grant = requested_role_ids
            .map(|role_ids| authorized_role_grant(role_ids, policy_revision, &roles))
            .transpose()?;
        Ok(AuthorizedAccountManagement {
            policy_revision,
            role_grant,
        })
    }

    /// 校验待写入角色权限不超过操作人当前权限。
    async fn authorize_permissions(
        &self,
        actor: &AuditActor,
        permissions: Vec<Permission>,
    ) -> Result<AuthorizedPermissions> {
        let permissions = PermissionSet::new(permissions);
        let enforcer = self.fresh_enforcer().await?.read().await;
        let actor_permissions = permissions_for_actor(&enforcer, actor)?;
        ensure_permission_subset(&actor_permissions, &permissions)?;
        Ok(AuthorizedPermissions {
            permissions,
            policy_revision: self.loaded_policy_revision.load(Ordering::Acquire),
        })
    }

    /// 批量加载目标账号当前角色和可选待分配角色。
    async fn load_management_roles(
        &self,
        target_role_ids: &[String],
        requested_role_ids: Option<&[String]>,
    ) -> Result<HashMap<String, Role>> {
        let mut role_ids = target_role_ids.to_vec();
        if let Some(requested_role_ids) = requested_role_ids {
            role_ids.extend_from_slice(requested_role_ids);
        }
        role_ids.sort();
        role_ids.dedup();
        Ok(self
            .db
            .roles()
            .roles_by_ids(&role_ids)
            .await?
            .into_iter()
            .map(|role| (role.base.id.clone(), role))
            .collect())
    }

    /// 校验操作人可管理角色的当前权限范围，并按需校验更新后的权限。
    async fn authorize_role_update(
        &self,
        actor: &AuditActor,
        role_id: &str,
        updated_permissions: Option<Vec<Permission>>,
    ) -> Result<AuthorizedRoleUpdate> {
        let permissions = updated_permissions.map(PermissionSet::new);
        let enforcer = self.fresh_enforcer().await?.read().await;
        let actor_permissions = permissions_for_actor(&enforcer, actor)?;
        let current_permissions = implicit_permissions_for_role(&enforcer, role_id)?;
        ensure_management_subset(&actor_permissions, &current_permissions)?;
        if let Some(permissions) = permissions.as_ref() {
            ensure_permission_subset(&actor_permissions, permissions)?;
        }
        Ok(AuthorizedRoleUpdate {
            permissions,
            policy_revision: self.loaded_policy_revision.load(Ordering::Acquire),
        })
    }

    /// 使用调用方 MongoDB 事务覆盖账号角色绑定。
    ///
    /// 该方法仅写入事务，不更新本地 Enforcer。调用方必须通过
    /// [`Self::run_authorized_policy_transaction`] 建立取消安全的事务与刷新边界。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `account_id` - 账号 ID
    /// * `grant` - 已按操作人权限和 policy 版本校验的授予上下文
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 事务内角色校验和绑定替换成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当角色无效或 MongoDB policy 写入失败时返回错误。
    pub(crate) async fn assign_roles_with_session(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        grant: AuthorizedRoleGrant,
        session: &mut ClientSession,
    ) -> Result<()> {
        let role_ids = grant.role_ids;
        self.ensure_roles_assignable_with_session(&role_ids, session)
            .await?;
        let role_keys = role_ids
            .iter()
            .map(|role_id| role_key(role_id))
            .collect::<Vec<_>>();
        self.policy_store
            .replace_subject_roles_with_session(&subject(account_kind, account_id), &role_keys, session)
            .await?;
        Ok(())
    }

    /// 使用调用方事务执行系统初始化角色绑定。
    ///
    /// 仅超级管理员初始化可调用；普通管理入口必须使用
    /// [`Self::assign_roles_with_session`] 的授权上下文。
    pub(in crate::iam) async fn assign_system_roles_with_session(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        role_ids: Vec<String>,
        session: &mut ClientSession,
    ) -> Result<()> {
        let role_ids = RoleIdSet::parse(role_ids)?.to_strings();
        self.ensure_roles_assignable_with_session(&role_ids, session)
            .await?;
        let role_keys = role_ids
            .iter()
            .map(|role_id| role_key(role_id))
            .collect::<Vec<_>>();
        self.policy_store
            .replace_subject_roles_with_session(&subject(account_kind, account_id), &role_keys, session)
            .await?;
        Ok(())
    }

    /// 使用调用方 MongoDB 事务清除账号全部角色绑定。
    ///
    /// 该方法仅写入事务，不更新本地 Enforcer。调用方必须通过
    /// [`Self::run_authorized_policy_transaction`] 建立取消安全的事务与刷新边界。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `account_id` - 账号 ID
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 事务内角色绑定清除成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当 MongoDB policy 删除失败时返回错误。
    pub(crate) async fn clear_roles_with_session(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        session: &mut ClientSession,
    ) -> Result<()> {
        self.policy_store
            .clear_subject_roles_with_session(&subject(account_kind, account_id), session)
            .await?;
        Ok(())
    }

    /// 查询账号直接绑定的角色 ID。
    ///
    /// # 错误
    /// 当 Casbin policy 加载失败时返回错误。
    pub(crate) async fn role_ids(&self, account_kind: AccountKind, account_id: &str) -> Result<Vec<String>> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        Ok(role_ids_for_account(&enforcer, account_kind, account_id))
    }

    /// 批量查询同类账号直接绑定的角色 ID。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_ids` - 待查询的账号 ID 集合。
    ///
    /// # 返回值
    /// 返回账号 ID 到角色 ID 集合的映射；未绑定角色的账号对应空集合。
    ///
    /// # 错误
    /// 当 Casbin policy 加载失败时返回错误。
    pub(crate) async fn role_ids_by_accounts(
        &self,
        account_kind: AccountKind,
        account_ids: &[String],
    ) -> Result<HashMap<String, Vec<String>>> {
        if account_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let enforcer = self.fresh_enforcer().await?.read().await;
        Ok(collect_role_ids(&enforcer, account_kind, account_ids))
    }

    /// 查询账号通过角色继承获得的权限。
    ///
    /// # 错误
    /// 当 Casbin policy 包含非法权限时返回错误。
    pub(crate) async fn permissions(
        &self,
        account_kind: AccountKind,
        account_id: &str,
    ) -> Result<Vec<Permission>> {
        let policies = self
            .fresh_enforcer()
            .await?
            .read()
            .await
            .get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
        parse_policy_permissions(policies)
    }

    /// 在调用方事务内校验并 CAS touch 全部待分配角色。
    ///
    /// 写触碰会与并发角色更新或删除产生写冲突，避免纯快照读取允许已删除角色被绑定。
    async fn ensure_roles_assignable_with_session(
        &self,
        role_ids: &[String],
        session: &mut ClientSession,
    ) -> Result<()> {
        let mut roles = self
            .db
            .roles()
            .enabled_roles_with_session(role_ids, session)
            .await?;
        ensure_all_roles_assignable(role_ids.len(), roles.len())?;
        for role in &mut roles {
            self.db.roles().update_with_session(role, session).await?;
        }
        Ok(())
    }

    /// 在取消安全的所有权任务内运行系统初始化 policy 事务。
    ///
    /// 该入口没有操作人授权快照，只允许内建角色和超级管理员初始化使用。
    ///
    /// # 参数
    /// * `transaction` - 在 MongoDB 会话中执行的事务函数
    ///
    /// # 返回值
    /// 返回事务函数的结果。
    ///
    /// # 错误
    /// 当事务、policy 刷新或所有权任务失败时返回错误。
    pub(in crate::iam) async fn run_system_policy_transaction<T, F>(
        self: &Arc<Self>,
        transaction: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut ClientSession) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send
            + 'static,
    {
        self.run_policy_transaction_at_revision(None, transaction).await
    }

    /// 使用授权快照对应的 policy 版本运行写事务。
    ///
    /// # 错误
    /// 授权后 policy 已变化、事务失败或本地刷新失败时返回错误。
    pub(crate) async fn run_authorized_policy_transaction<T, F>(
        self: &Arc<Self>,
        policy_revision: u64,
        transaction: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut ClientSession) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send
            + 'static,
    {
        self.run_policy_transaction_at_revision(Some(policy_revision), transaction)
            .await
    }

    /// 运行 policy 写事务，并按需绑定事务前授权使用的版本。
    async fn run_policy_transaction_at_revision<T, F>(
        self: &Arc<Self>,
        expected_revision: Option<u64>,
        transaction: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut ClientSession) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send
            + 'static,
    {
        let client = self.db.client().clone();
        self.ensure_policy_consistency_known()?;
        let policy_write = self.policy_write.clone().lock_owned().await;
        let rbac = self.clone();
        await_owned("RBAC", async move {
            let _policy_write = policy_write;
            rbac.ensure_policy_consistency_known()?;
            let policy_store = rbac.policy_store.clone();
            let result = client
                .with_transaction(move |session| {
                    Box::pin(async move {
                        let value = transaction(session).await?;
                        match expected_revision {
                            Some(revision) => {
                                policy_store
                                    .bump_policy_revision_if_matches_with_session(revision, session)
                                    .await?;
                            }
                            None => policy_store.bump_policy_revision_with_session(session).await?,
                        }
                        Ok(value)
                    })
                })
                .await;
            rbac.finish_policy_transaction(result).await
        })
        .await
    }

    /// 使用授权快照对应版本运行 policy 与审计原子事务。
    ///
    /// # 错误
    /// 业务写入、审计、policy 版本比较、提交或刷新失败时返回错误。
    pub(crate) async fn run_authorized_audited_policy_transaction<T, F>(
        self: &Arc<Self>,
        policy_revision: u64,
        audit: AuditLog,
        transaction: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&'a mut ClientSession) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send
            + 'static,
    {
        let db = self.db.clone();
        self.run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(async move {
                let value = transaction(session).await?;
                db.audit_logs().create_with_session(&audit, session).await?;
                Ok(value)
            })
        })
        .await
    }

    /// 完成 policy 事务，并在提交成功时刷新本地 Enforcer。
    ///
    /// 提交结果未知时进入不可自动恢复的一致性未知状态，避免单次旧快照 reload 误解锁。
    ///
    /// # 参数
    /// * `transaction` - 已完成的 MongoDB policy 事务结果
    /// # 返回值
    /// 成功时返回事务值；失败时返回原始事务错误。
    ///
    /// # 错误
    /// 事务提交失败时返回错误。提交成功后的刷新失败只记录错误并保持本地失败关闭，
    /// 避免把已提交写入误报为可安全重试的失败。
    async fn finish_policy_transaction<T>(&self, transaction: Result<T>) -> Result<T> {
        match transaction {
            Ok(value) => {
                if let Err(error) = self.refresh_policy().await {
                    tracing::error!(
                        error = %error,
                        committed = true,
                        "RBAC policy committed but local Enforcer refresh failed"
                    );
                }
                Ok(value)
            }
            Err(error) if commit_outcome_unknown(&error) => {
                self.policy_consistency_unknown.store(true, Ordering::Release);
                self.policy_stale.store(true, Ordering::Release);
                tracing::error!(
                    error = %error,
                    restart_required = true,
                    "RBAC policy commit outcome is unknown; authorization has been stopped"
                );
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    /// 在提交结果未知后阻止授权读取和后续 policy 写入，避免旧快照被误判为最新状态。
    fn ensure_policy_consistency_known(&self) -> Result<()> {
        if self.policy_consistency_unknown.load(Ordering::Acquire) {
            return Err(Error::Rbac(
                "授权策略提交结果未知，当前进程已停止授权，请重启服务".to_string(),
            ));
        }
        Ok(())
    }

    /// 使用指定 ID 在同一事务中创建角色实体并写入完整权限规则。
    ///
    /// 仅内建角色初始化可以指定固定 ID；普通角色始终由 [`Self::create_role`] 生成 ID。
    async fn create_role_with_id(
        self: &Arc<Self>,
        id: String,
        data: RoleData,
        permissions: Vec<Permission>,
        audit: Option<AuditLog>,
        expected_revision: Option<u64>,
    ) -> Result<Role> {
        let role = Role::new(id, data)?;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(permissions);
        self.run_policy_transaction_at_revision(expected_revision, move |session| {
            Box::pin(async move {
                db.roles().create_with_session(&role, session).await?;
                policy_store
                    .replace_role_permissions_with_session(&role_key, &permissions, session)
                    .await?;
                if let Some(audit) = audit {
                    db.audit_logs().create_with_session(&audit, session).await?;
                }
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 在同一事务中更新角色实体并覆盖完整权限规则。
    async fn update_role_with_permissions(
        self: &Arc<Self>,
        mut role: Role,
        name: String,
        permissions: PermissionSet,
        policy_revision: u64,
        audit: AuditLog,
    ) -> Result<Role> {
        role.update(RoleUpdate {
            name: Some(name),
            ..Default::default()
        })?;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(permissions.into_vec());
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
            Box::pin(async move {
                db.roles().update_with_session(&mut role, session).await?;
                policy_store
                    .replace_role_permissions_with_session(&role_key, &permissions, session)
                    .await?;
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 原子修复内建 root 角色元数据与完整权限。
    async fn repair_root_role(self: &Arc<Self>, mut role: Role, root_permission: Permission) -> Result<Role> {
        role.system = true;
        role.disabled = false;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(vec![root_permission]);
        self.run_policy_transaction_at_revision(None, move |session| {
            Box::pin(async move {
                if role.base.is_deleted() {
                    db.roles().restore_with_session(&mut role, session).await?;
                }
                db.roles().update_with_session(&mut role, session).await?;
                policy_store
                    .replace_role_permissions_with_session(&role_key, &permissions, session)
                    .await?;
                Ok(role)
            })
        })
        .await
    }

    /// 在同一事务中软删除角色实体及其权限和账号绑定。
    async fn delete_role_with_policy(
        self: &Arc<Self>,
        mut role: Role,
        audit: AuditLog,
        policy_revision: u64,
    ) -> Result<()> {
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
            Box::pin(async move {
                db.roles().soft_delete_with_session(&mut role, session).await?;
                policy_store.remove_role_with_session(&role_key, session).await?;
                Ok::<(), Error>(())
            })
        })
        .await
    }
}

/// 创建共享 RBAC 服务。
///
/// # 返回值
/// 返回延迟初始化 Casbin Enforcer 的共享服务。
pub fn shared_rbac_service(db: Database) -> SharedRbacService {
    Arc::new(RbacService::new(db))
}

/// 确保 root 角色存在且拥有全量权限。
///
/// # 错误
/// 当角色或 Casbin policy 写入失败时返回错误。
pub async fn ensure_root_role(rbac: &SharedRbacService) -> Result<Role> {
    let root_permission = Permission::parse("*:*")?;
    let mut attempts = 0;
    loop {
        attempts += 1;
        let result = ensure_root_role_once(rbac, &root_permission).await;
        match result {
            Err(Error::ConflictError(_)) if attempts < ROOT_ROLE_INIT_ATTEMPTS => continue,
            result => return result,
        }
    }
}

/// 执行一次可重入的 root 角色校验或修复。
async fn ensure_root_role_once(rbac: &SharedRbacService, root_permission: &Permission) -> Result<Role> {
    if let Some(role) = rbac.db.roles().find_by_id_including_deleted(ROOT_ROLE_ID).await? {
        let enforcer = rbac.fresh_enforcer().await?.read().await;
        let permissions = permissions_for_role(&enforcer, ROOT_ROLE_ID)?;
        drop(enforcer);
        if root_role_is_current(&role, &permissions, root_permission) {
            return Ok(role);
        }
        return rbac.repair_root_role(role, root_permission.clone()).await;
    }

    let data = RoleData {
        name: ROOT_ROLE_NAME.to_string(),
        description: None,
        system: true,
    };
    rbac.create_role_with_id(
        ROOT_ROLE_ID.to_string(),
        data,
        vec![root_permission.clone()],
        None,
        None,
    )
    .await
}

/// 构建 Casbin 主体标识。
///
/// # 返回值
/// 返回包含账号类型和账号 ID 的稳定主体标识。
pub fn subject(account_kind: AccountKind, account_id: &str) -> String {
    format!("user:{}:{account_id}", account_kind.as_str())
}

fn role_key(role_id: &str) -> String {
    format!("{ROLE_PREFIX}{role_id}")
}

fn permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<Vec<Permission>> {
    parse_policy_permissions(enforcer.get_permissions_for_user(&role_key(role_id), None))
}

fn implicit_permissions_for_role(enforcer: &Enforcer, role_id: &str) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&role_key(role_id), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

fn collect_role_permissions(
    enforcer: &Enforcer,
    role_ids: &[String],
) -> Result<HashMap<String, Vec<Permission>>> {
    role_ids
        .iter()
        .map(|role_id| Ok((role_id.clone(), permissions_for_role(enforcer, role_id)?)))
        .collect()
}

fn permissions_for_actor(enforcer: &Enforcer, actor: &AuditActor) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(actor.kind(), actor.id()), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

fn permissions_for_account(
    enforcer: &Enforcer,
    account_kind: AccountKind,
    account_id: &str,
) -> Result<PermissionSet> {
    let policies = enforcer.get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
    parse_policy_permissions(policies).map(PermissionSet::new)
}

fn permissions_for_roles(enforcer: &Enforcer, role_ids: &[String]) -> Result<PermissionSet> {
    let mut permissions = Vec::new();
    for role_id in role_ids {
        permissions.extend(implicit_permissions_for_role(enforcer, role_id)?.into_vec());
    }
    Ok(PermissionSet::new(permissions))
}

fn role_ids_for_account(enforcer: &Enforcer, account_kind: AccountKind, account_id: &str) -> Vec<String> {
    enforcer
        .get_roles_for_user(&subject(account_kind, account_id), None)
        .into_iter()
        .filter_map(|role| role.strip_prefix(ROLE_PREFIX).map(str::to_string))
        .collect()
}

fn collect_role_ids(
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

fn parse_policy_permissions(policies: Vec<Vec<String>>) -> Result<Vec<Permission>> {
    let mut permissions = Vec::new();
    for policy in policies {
        let [_, resource, action] = policy.as_slice() else {
            continue;
        };
        permissions.push(Permission::parse(format!("{resource}:{action}"))?);
    }
    Ok(PermissionSet::new(permissions).into_vec())
}

fn permission_pairs(permissions: Vec<Permission>) -> Vec<(String, String)> {
    PermissionSet::new(permissions)
        .into_vec()
        .into_iter()
        .map(|permission| (permission.resource().to_string(), permission.action().to_string()))
        .collect()
}

/// 校验事务内解析出的可分配角色数量与请求一致。
fn ensure_all_roles_assignable(requested_count: usize, existing_count: usize) -> Result<()> {
    if existing_count != requested_count {
        return Err(Error::BusinessLogicError("角色不存在或已停用".to_string()));
    }
    Ok(())
}

fn ensure_roles_delegable(roles: &[Role]) -> Result<()> {
    roles.iter().try_for_each(|role| {
        if !role_is_assignable(role) {
            return Err(Error::Forbidden(
                "系统角色或已停用角色不能通过普通接口分配".to_string(),
            ));
        }
        Ok(())
    })
}

fn role_is_assignable(role: &Role) -> bool {
    role.base.id != ROOT_ROLE_ID && role.ensure_assignable().is_ok()
}

fn ensure_role_mutable(role: &Role) -> Result<()> {
    if role.base.id == ROOT_ROLE_ID || role.ensure_mutable().is_err() {
        return Err(Error::Forbidden("系统角色不能修改".to_string()));
    }
    Ok(())
}

fn ensure_role_deletable(role: &Role) -> Result<()> {
    if role.base.id == ROOT_ROLE_ID || role.ensure_deletable().is_err() {
        return Err(Error::Forbidden("系统角色不能删除".to_string()));
    }
    Ok(())
}

fn authorize_account_permissions(
    enforcer: &Enforcer,
    actor_permissions: &PermissionSet,
    target_kind: AccountKind,
    target_id: &str,
    requested_role_ids: Option<&[String]>,
) -> Result<()> {
    let target_permissions = permissions_for_account(enforcer, target_kind, target_id)?;
    ensure_management_subset(actor_permissions, &target_permissions)?;
    if let Some(role_ids) = requested_role_ids {
        let requested_permissions = permissions_for_roles(enforcer, role_ids)?;
        ensure_permission_subset(actor_permissions, &requested_permissions)?;
    }
    Ok(())
}

fn ensure_target_roles_manageable(target_role_ids: &[String], roles: &HashMap<String, Role>) -> Result<()> {
    let protected = target_role_ids
        .iter()
        .any(|role_id| role_id == ROOT_ROLE_ID || roles.get(role_id).is_none_or(|role| role.system));
    if protected {
        return Err(Error::Forbidden(
            "绑定系统角色的账号不能通过普通管理接口修改".to_string(),
        ));
    }
    Ok(())
}

fn authorized_role_grant(
    role_ids: Vec<String>,
    policy_revision: u64,
    roles: &HashMap<String, Role>,
) -> Result<AuthorizedRoleGrant> {
    let requested_roles = role_ids
        .iter()
        .filter_map(|role_id| roles.get(role_id))
        .cloned()
        .collect::<Vec<_>>();
    ensure_all_roles_assignable(role_ids.len(), requested_roles.len())?;
    ensure_roles_delegable(&requested_roles)?;
    Ok(AuthorizedRoleGrant {
        role_ids,
        policy_revision,
    })
}

fn ensure_permission_subset(actor: &PermissionSet, required: &PermissionSet) -> Result<()> {
    if !actor.covers(required) {
        return Err(Error::Forbidden(
            "不能授予超出自身权限范围的角色或权限".to_string(),
        ));
    }
    Ok(())
}

fn ensure_management_subset(actor: &PermissionSet, target: &PermissionSet) -> Result<()> {
    if !actor.covers(target) {
        return Err(Error::Forbidden(
            "不能管理权限范围高于自身的账号或角色".to_string(),
        ));
    }
    Ok(())
}

fn role_or_not_found(role: Option<Role>) -> Result<Role> {
    role.ok_or_else(|| Error::NotFound("角色不存在".to_string()))
}

fn commit_outcome_unknown(error: &Error) -> bool {
    matches!(error, Error::OutcomeUnknown(_))
}

fn stable_policy_revision(before: u64, after: u64) -> Option<u64> {
    (before == after).then_some(after)
}

fn root_role_is_current(role: &Role, permissions: &[Permission], root_permission: &Permission) -> bool {
    !role.base.is_deleted()
        && role.system
        && !role.disabled
        && permissions == std::slice::from_ref(root_permission)
}

fn policy_revisions_match(loaded: u64, database: u64) -> bool {
    loaded == database
}

fn rbac_error(error: impl std::fmt::Display) -> Error {
    Error::Rbac(error.to_string())
}

#[cfg(test)]
mod tests {
    use casbin::{CoreApi, Enforcer, MemoryAdapter, RbacApi};
    use entities::{AccountKind, Permission, PermissionSet, Role, RoleData};

    use super::{
        collect_role_ids, collect_role_permissions, commit_outcome_unknown, ensure_all_roles_assignable,
        ensure_management_subset, ensure_permission_subset, ensure_role_deletable, ensure_role_mutable,
        ensure_roles_delegable, ensure_target_roles_manageable, parse_policy_permissions, permission_pairs,
        permissions_for_roles, policy_revisions_match, role_key, role_or_not_found, root_role_is_current,
        stable_policy_revision, RbacService, RBAC_MODEL, ROOT_ROLE_ID,
    };
    use crate::errors::Error;

    #[test]
    fn commit_outcome_unknown_is_detected_separately_from_definite_errors() {
        let unknown = Error::from(database::Error::CommitOutcomeUnknown(
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
        let unknown = Error::from(database::Error::CommitOutcomeUnknown(
            mongodb::error::Error::custom("unknown"),
        ));

        let result = rbac.finish_policy_transaction::<()>(Err(unknown)).await;

        assert!(matches!(
            result,
            Err(Error::OutcomeUnknown(database::Error::CommitOutcomeUnknown(_)))
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
}
