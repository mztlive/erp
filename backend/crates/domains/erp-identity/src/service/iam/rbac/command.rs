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
    /// 创建角色并写入 Casbin 权限策略。
    /// 返回创建后的角色实体。
    ///
    /// 当角色校验、持久化或 Casbin policy 写入失败时返回错误。
    ///
    /// authorization 为应用注入的共享授权服务。
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

    // 在原 policy 事务内核验创建入口，再写入角色、权限与审计。
    // 原子持久化后的角色。
    //
    // 入口拒绝、权限版本变化或角色与策略写入失败时回滚。
    //
    // role、permissions 为通过子集校验的目标，audit、revision 为原授权版本，access 为当前管理入口。
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

    /// 更新角色信息并按需覆盖权限。
    /// 返回更新后的角色实体。
    ///
    /// 当角色不存在、校验失败或持久化失败时返回错误。
    ///
    /// authorization 为应用注入的共享授权服务。
    /// `params`：可选名称与权限更新。
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

    /// 删除非系统角色及其 Casbin policy/绑定。
    /// 角色实体与关联 policy 原子删除成功时返回 `Ok(())`。
    ///
    /// 当角色不存在、角色为系统角色或删除失败时返回错误。
    ///
    /// authorization 为应用注入的共享授权服务。
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

    // 当角色编辑请求成功但没有字段需要改动时，仍保存该次操作的审计记录。
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

    // 只修改角色属性的命令在一个事务内保存角色更新及审计，不改写角色权限集合。
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

    // 在同一事务中 CAS touch 角色并覆盖权限。
    //
    // 角色写触碰会递增版本与更新时间，使权限更新、角色绑定和角色删除在多实例部署下
    // 对同一角色产生写冲突，避免删除后遗留 policy 引用。
    //
    // 返回写触碰后的角色。
    //
    // 角色不存在、policy 版本不匹配或角色、权限、审计写入失败时返回错误。
    //
    // `permissions`：替换后的完整直接权限；`audit`：可选的已准备资源审计；有值时与权限写入同一事务。
    // `expected_revision`：授权快照要求的 policy 版本；为空时不比较版本。
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

    // 使用指定 ID 在同一事务中创建角色实体并写入完整权限规则。
    //
    // 仅内建角色初始化或经授权的显式种子命令可以指定固定 ID；普通管理入口由 `Self::create_role` 生成 ID。
    //
    // 返回已创建的角色。
    //
    // 角色数据不合法、ID 冲突、policy 版本不匹配或权限、审计写入失败时返回错误。
    //
    // `permissions`：首次写入的完整直接权限；`audit`：可选的已准备资源审计。
    // `expected_revision`：授权快照要求的 policy 版本；为空时不比较版本。
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

    // 读取角色自身的直接权限，不展开继承关系。
    // 返回该角色的直接权限集。
    //
    // Enforcer 无法刷新，或策略行无法解析为权限时返回错误。
    pub(super) async fn direct_role_permissions(&self, role_id: &str) -> Result<PermissionSet> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        permissions_for_role(&enforcer, role_id)
    }

    // 角色编辑同时修改权限时，在同一个事务保存角色实体并替换整套权限规则，避免部分更新。
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

    // 原子修复内建 root 角色元数据与完整权限。
    // 返回已恢复为系统角色、启用且只保留该权限的角色。
    //
    // 角色恢复、更新或权限替换失败时返回错误。
    //
    // `role`：当前 root 角色，允许处于软删除状态；`root_permission`：要写回的 `*:*` 权限。
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

    // 提交角色删除时，在一个策略事务内软删角色并清理其权限规则及账号绑定，避免留下仍生效的策略。
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
