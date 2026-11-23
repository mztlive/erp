use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::admin;
use persistence_core::{Executor, NoTransaction, ResetRetention};

use super::policy::{
    permission_pairs, permissions_for_role, role_key, role_or_not_found, root_role_is_current,
};
use super::{
    AuthorizedRoleUpdate, ROOT_ROLE_ID, ROOT_ROLE_INIT_ATTEMPTS, ROOT_ROLE_NAME, RbacService,
    SharedRbacService,
};
use crate::dto::{CreateRoleParams, UpdateRoleParams};
use crate::entity::rbac::{Permission, PermissionSet};
use crate::entity::role::{Role, RoleData, RoleUpdate};
use crate::error::{Error, Result};
use crate::ports::PreparedResourceAudit;
use crate::repository::database_reset;
use crate::service::iam::management::ManagementAccess;
use crate::{AccessControlExt, AuthorizationService, RoleId};

impl RbacService {
    /// 普通管理入口通过共享授权服务预检创建资格和授予上限，再生成角色 ID。
    /// 在策略事务内复核入口并原子写入角色、直接权限与审计；校验或写入失败时返回错误。
    pub async fn create_role(
        self: &Arc<Self>,
        params: CreateRoleParams,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<Role> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ROLE_CREATE_ROLE);
        access.preflight(&self.db).await?;
        let authorized = self.authorize_permissions(&actor, params.permissions).await?;

        let id = id_generator::next_id();
        let audit = self.audit.resource_log(actor, "role.create", "role", id.clone())?;
        let role = Role::new(
            RoleId::parse(id)?,
            RoleData { workbenches: params.workbenches, ..RoleData::new(params.name) },
        )?;

        self.create_managed_role(role, authorized.permissions, audit, authorized.policy_revision, access)
            .await
    }

    /// 管理入口或显式种子命令预检后，接收通过子集校验的角色和权限，并在指定版本的策略事务内复核创建入口。
    /// 角色、直接权限与预备审计一并提交；入口拒绝、版本变化或任一写入失败时回滚。
    pub(super) async fn create_managed_role(
        self: &Arc<Self>,
        role: Role,
        permissions: PermissionSet,
        audit: PreparedResourceAudit,
        revision: u64,
        access: ManagementAccess,
    ) -> Result<Role> {
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let key = role_key(&role.base.id);
        let permissions = permission_pairs(permissions.into_vec());
        self.run_authorized_audited_policy_transaction(revision, audit, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;
                db.roles().create(&role, executor).await?;
                policy_store.replace_role_permissions(&key, &permissions, executor).await?;
                Ok(role)
            })
        })
        .await
    }

    /// 普通管理入口通过共享授权服务预检编辑资格，校验角色可修改及现有和拟授予权限均在管理上限内。
    /// 按 `params` 更新名称、工作台或整套直接权限；空更新也写审计。角色不存在、校验或持久化失败时返回错误。
    pub async fn update_role(
        self: &Arc<Self>,
        id: &str,
        params: UpdateRoleParams,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<Role> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ROLE_UPDATE_ROLE);
        access.preflight(&self.db).await?;

        let mut role = role_or_not_found(self.db.roles().find_by_id(id, &mut NoTransaction).await?)?;
        role.ensure_mutable().map_err(|_| Error::Forbidden("系统角色不能修改".into()))?;
        let AuthorizedRoleUpdate { permissions, policy_revision } =
            self.authorize_role_update(&actor, role.base.id.as_str(), params.permissions).await?;
        let audit = self.audit.resource_log(actor, "role.update", "role", role.base.id.clone())?;

        let attr_changed = params.name.is_some() || params.workbenches.is_some();
        if attr_changed {
            role.update(RoleUpdate {
                name: params.name,
                workbenches: params.workbenches,
                ..Default::default()
            })?;
        }
        let permissions = permissions.map(|set| permission_pairs(set.into_vec()));
        let touch_role = attr_changed || permissions.is_some();
        self.update_role_with_policy(role, touch_role, permissions, audit, policy_revision, access).await
    }

    /// 普通管理入口通过共享授权服务预检删除资格，排除内建 `role-root` 和系统角色，并校验现有权限在管理上限内。
    /// 在策略事务中复核入口，原子软删角色、清理权限与绑定并写审计；角色不存在、校验或写入失败时返回错误。
    pub async fn delete_role(
        self: &Arc<Self>,
        id: &str,
        actor: AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<()> {
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ROLE_DELETE_ROLE);
        access.preflight(&self.db).await?;
        let role = role_or_not_found(self.db.roles().find_by_id(id, &mut NoTransaction).await?)?;
        role.ensure_deletable().map_err(|_| Error::Forbidden("系统角色不能删除".into()))?;
        let authorized = self.authorize_role_update(&actor, role.base.id.as_str(), None).await?;
        let audit = self.audit.resource_log(actor, "role.delete", "role", role.base.id.clone())?;

        self.delete_role_with_policy(role, audit, authorized.policy_revision, access).await
    }

    // 在策略事务内复核入口，按需更新角色版本/属性、覆盖直接权限并持久化审计；
    // 仅改权限时同样更新角色版本，使跨实例权限更新、角色绑定与删除产生写冲突。
    async fn update_role_with_policy(
        self: &Arc<Self>,
        mut role: Role,
        touch_role: bool,
        permissions: Option<Vec<(String, String)>>,
        audit: PreparedResourceAudit,
        policy_revision: u64,
        access: ManagementAccess,
    ) -> Result<Role> {
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;
                if touch_role {
                    db.roles().update(&mut role, executor).await?;
                }
                if let Some(permissions) = permissions {
                    policy_store.replace_role_permissions(&role_key, &permissions, executor).await?;
                }
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 内建角色初始化按固定 ID 写入角色及完整直接权限；普通管理入口由 [`Self::create_role`] 生成 ID。
    pub(super) async fn create_role_with_id(
        self: &Arc<Self>,
        id: String,
        data: RoleData,
        permissions: Vec<Permission>,
    ) -> Result<Role> {
        let role = Role::new(RoleId::parse(id)?, data)?;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(permissions);
        self.run_policy_transaction_at_revision(None, move |executor| {
            Box::pin(async move {
                db.roles().create(&role, executor).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, executor).await?;
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 补齐已有种子角色前，从最新快照读取角色自身的直接权限，继承权限不计入。
    pub(super) async fn direct_role_permissions(&self, role_id: &str) -> Result<PermissionSet> {
        let snapshot = self.fresh_snapshot().await?.read().await;
        permissions_for_role(&snapshot, role_id)
    }

    /// 初始化发现内建 `role-root` 偏离预期时，在同一事务按需恢复软删除记录，设为启用的系统角色并仅保留直接权限 `*:*`。
    pub(super) async fn repair_root_role(
        self: &Arc<Self>,
        mut role: Role,
        root_permission: Permission,
    ) -> Result<Role> {
        role.system = true;
        role.disabled = false;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(vec![root_permission]);
        self.run_policy_transaction_at_revision(None, move |executor| {
            Box::pin(async move {
                if role.base.is_deleted() {
                    db.roles().restore(&mut role, executor).await?;
                }
                db.roles().update(&mut role, executor).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, executor).await?;
                Ok(role)
            })
        })
        .await
    }

    // 删除预检后，在策略事务内复核入口，原子软删角色、清理权限与账号绑定并写审计，避免策略残留。
    async fn delete_role_with_policy(
        self: &Arc<Self>,
        mut role: Role,
        audit: PreparedResourceAudit,
        policy_revision: u64,
        access: ManagementAccess,
    ) -> Result<()> {
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;
                db.roles().soft_delete(&mut role, executor).await?;
                policy_store.remove_role(&role_key, executor).await?;
                Ok::<(), Error>(())
            })
        })
        .await
    }

    /// 全库重置预检要求已认证操作人的登录名为 `admin`，且其直接绑定的内建 `role-root` 角色授予 `*:*`。
    /// 通过时返回策略快照版本供重置事务比对，账号与角色的有效状态由事务内复核。
    pub async fn authorize_database_reset(&self, actor: &AuditActor) -> Result<u64> {
        if actor.account() != "admin" {
            return Err(Error::Forbidden("仅 admin 超级管理员可以清空演示数据库".into()));
        }
        let permission = Permission::parse("*:*")?;
        let snapshot = self
            .role_permission_snapshot(actor.kind(), actor.id(), std::slice::from_ref(&permission))
            .await?;
        if !snapshot.granting_role_ids(&permission).iter().any(|role| role.as_str() == ROOT_ROLE_ID) {
            return Err(Error::Forbidden("清空数据库需要内建超级管理员权限".into()));
        }
        Ok(snapshot.policy_revision())
    }

    /// 在重置事务内、删除前复核发起人是仍有效的 `admin` 本人且内建 `role-root` 角色仍启用，并返回须保留文档集合。
    pub async fn database_reset_retention(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ResetRetention> {
        database_reset::retention(&self.db, actor, executor).await
    }
}

/// 启动或初始化超级管理员时，按固定 ID `role-root` 创建或修复角色及 `*:*` 策略，返回生效角色。
pub async fn ensure_root_role(rbac: &SharedRbacService) -> Result<Role> {
    let root_permission = Permission::parse("*:*")?;
    let mut attempts = 0;
    loop {
        attempts += 1;
        match ensure_root_role_once(rbac, &root_permission).await {
            Err(Error::ConflictError(_)) if attempts < ROOT_ROLE_INIT_ATTEMPTS => continue,
            result => return result,
        }
    }
}

async fn ensure_root_role_once(rbac: &SharedRbacService, root_permission: &Permission) -> Result<Role> {
    if let Some(role) = rbac.db.roles().find_by_id_including_deleted(ROOT_ROLE_ID, &mut NoTransaction).await?
    {
        let snapshot = rbac.fresh_snapshot().await?.read().await;
        let permissions = permissions_for_role(&snapshot, ROOT_ROLE_ID)?;
        drop(snapshot);
        if root_role_is_current(&role, permissions.as_slice(), root_permission) {
            return Ok(role);
        }
        return rbac.repair_root_role(role, root_permission.clone()).await;
    }

    let data = RoleData::new(ROOT_ROLE_NAME.to_string()).with_system(true);
    rbac.create_role_with_id(ROOT_ROLE_ID.to_string(), data, vec![root_permission.clone()]).await
}
