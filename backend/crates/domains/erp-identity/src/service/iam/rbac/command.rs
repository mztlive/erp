use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::admin;
use persistence_core::NoTransaction;

use super::policy::{permission_pairs, permissions_for_role, role_key, role_or_not_found};
use super::{AuthorizedRoleUpdate, RbacService};
use crate::dto::{CreateRoleParams, UpdateRoleParams};
use crate::entity::rbac::{Permission, PermissionSet};
use crate::entity::role::{Role, RoleData, RoleUpdate};
use crate::error::{Error, Result};
use crate::ports::PreparedResourceAudit;
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

        let role = role_or_not_found(self.db.roles().find_by_id(id, &mut NoTransaction).await?)?;
        role.ensure_mutable().map_err(|_| Error::Forbidden("系统角色不能修改".into()))?;
        let authorized =
            self.authorize_role_update(&actor, role.base.id.as_str(), params.permissions).await?;
        let audit = self.audit.resource_log(actor, "role.update", "role", role.base.id.clone())?;
        let AuthorizedRoleUpdate { permissions, policy_revision } = authorized;

        let update = RoleUpdate { name: params.name, workbenches: params.workbenches, ..Default::default() };
        match (update.name.is_some() || update.workbenches.is_some(), permissions) {
            (false, None) => self.audit_role_update(role, audit, policy_revision, access).await,
            (true, None) => self.update_role_attributes(role, update, audit, policy_revision, access).await,
            (false, Some(permissions)) => {
                self.replace_role_permissions(
                    &role.base.id,
                    permissions.into_vec(),
                    Some(audit),
                    Some(policy_revision),
                    access,
                )
                .await
            },
            (true, Some(permissions)) => {
                self.update_role_with_permissions(role, update, (permissions, policy_revision), audit, access)
                    .await
            },
        }
    }

    /// 普通管理入口通过共享授权服务预检删除资格，排除内建 `role-root` 和系统角色，并校验现有权限在管理上限内。
    /// 在策略事务中复核入口，原子软删角色、清理 Casbin 权限与绑定并写审计；角色不存在、校验或写入失败时返回错误。
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

    // 编辑请求未提供任何改动时，仍在策略事务内复核编辑入口、保存成功审计，并返回原角色。
    async fn audit_role_update(
        self: &Arc<Self>,
        role: Role,
        audit: PreparedResourceAudit,
        policy_revision: u64,
        access: ManagementAccess,
    ) -> Result<Role> {
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;
                Ok(role)
            })
        })
        .await
    }

    // 编辑仅涉及名称或工作台时，复核事务内入口并保存角色和审计，保留现有权限集合。
    async fn update_role_attributes(
        self: &Arc<Self>,
        mut role: Role,
        update: RoleUpdate,
        audit: PreparedResourceAudit,
        policy_revision: u64,
        access: ManagementAccess,
    ) -> Result<Role> {
        role.update(update)?;
        let db = self.db.clone();
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;
                db.roles().update(&mut role, executor).await?;
                Ok(role)
            })
        })
        .await
    }

    /// 仅替换权限时，在同一事务内复核管理入口、按版本更新角色并覆盖整套直接权限，按需保存预备审计。
    /// 更新角色版本和时间戳，让跨实例的权限更新、角色绑定与角色删除产生写冲突，避免残留策略引用。
    /// 返回更新后的角色；`expected_revision` 有值时须匹配授权版本，角色缺失、版本冲突或写入失败时返回错误。
    pub(super) async fn replace_role_permissions(
        self: &Arc<Self>,
        role_id: &str,
        permissions: Vec<Permission>,
        audit: Option<PreparedResourceAudit>,
        expected_revision: Option<u64>,
        access: ManagementAccess,
    ) -> Result<Role> {
        let role_id = role_id.to_string();
        let role_key = role_key(&role_id);
        let permissions = permission_pairs(permissions);
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let audit_port = self.audit.clone();
        self.run_policy_transaction_at_revision(expected_revision, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;

                let mut role = role_or_not_found(db.roles().find_by_id(&role_id, executor).await?)?;
                db.roles().update(&mut role, executor).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, executor).await?;

                if let Some(audit) = audit {
                    audit_port.persist(&audit, executor).await?;
                }
                Ok(role)
            })
        })
        .await
    }

    /// 内建角色初始化或经授权的显式种子命令可指定固定 ID；普通管理入口由 [`Self::create_role`] 生成 ID。
    /// 在同一事务写入角色、完整直接权限及可选的预备审计；`expected_revision` 有值时须匹配授权版本。
    /// 角色数据无效、ID 冲突、版本不符或写入失败时返回错误。
    pub(super) async fn create_role_with_id(
        self: &Arc<Self>,
        id: String,
        data: RoleData,
        permissions: Vec<Permission>,
        audit: Option<PreparedResourceAudit>,
        expected_revision: Option<u64>,
    ) -> Result<Role> {
        let role = Role::new(RoleId::parse(id)?, data)?;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let audit_port = self.audit.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(permissions);
        self.run_policy_transaction_at_revision(expected_revision, move |executor| {
            Box::pin(async move {
                db.roles().create(&role, executor).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, executor).await?;

                if let Some(audit) = audit {
                    audit_port.persist(&audit, executor).await?;
                }
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 补齐已有种子角色前，从最新 Enforcer 读取角色自身的直接权限，继承权限不计入。
    /// 策略刷新或权限解析失败时返回错误。
    pub(super) async fn direct_role_permissions(&self, role_id: &str) -> Result<PermissionSet> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        permissions_for_role(&enforcer, role_id)
    }

    // 编辑同时涉及角色属性和权限时，复核事务内入口，一并保存角色、整套直接权限及审计。
    async fn update_role_with_permissions(
        self: &Arc<Self>,
        mut role: Role,
        update: RoleUpdate,
        authorized: (PermissionSet, u64),
        audit: PreparedResourceAudit,
        access: ManagementAccess,
    ) -> Result<Role> {
        let (permissions, policy_revision) = authorized;
        role.update(update)?;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(permissions.into_vec());
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |executor| {
            Box::pin(async move {
                access.require(executor).await?;
                db.roles().update(&mut role, executor).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, executor).await?;
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 初始化发现内建 `role-root` 偏离预期时，在同一事务按需恢复软删除记录，设为启用的系统角色并仅保留直接权限 `*:*`。
    /// 恢复、角色更新或权限替换失败时返回错误。
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
}
