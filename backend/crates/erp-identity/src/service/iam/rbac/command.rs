use std::sync::Arc;

use application_core::AuditActor;
use persistence_core::NoTransaction;

use super::authorize::{ensure_role_deletable, ensure_role_mutable};
use super::policy::{permission_pairs, permissions_for_role, role_key, role_or_not_found};
use super::{AuthorizedRoleUpdate, RbacService};
use crate::AccessControlExt;
use crate::dto::{CreateRoleParams, UpdateRoleParams};
use crate::entity::rbac::{Permission, PermissionSet};
use crate::entity::role::{Role, RoleData, RoleUpdate};
use crate::error::{Error, Result};
use crate::ports::PreparedResourceAudit;

impl RbacService {
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
        let audit = self.audit.resource_log(actor, "role.create", "role", id.clone())?;
        self.create_role_with_id(
            id,
            RoleData { name: params.name, description: None, system: false },
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
        let role = role_or_not_found(self.db.roles().find_by_id(id, &mut NoTransaction).await?)?;
        ensure_role_mutable(&role)?;
        let authorized =
            self.authorize_role_update(&actor, role.base.id.as_str(), params.permissions).await?;
        let audit = self.audit.resource_log(actor, "role.update", "role", role.base.id.clone())?;
        let AuthorizedRoleUpdate { permissions, policy_revision } = authorized;

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
            },
            (Some(name), Some(permissions)) => {
                self.update_role_with_permissions(role, name, permissions, policy_revision, audit).await
            },
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
        let role = role_or_not_found(self.db.roles().find_by_id(id, &mut NoTransaction).await?)?;
        ensure_role_deletable(&role)?;
        let authorized = self.authorize_role_update(&actor, role.base.id.as_str(), None).await?;
        let audit = self.audit.resource_log(actor, "role.delete", "role", role.base.id.clone())?;

        self.delete_role_with_policy(role, audit, authorized.policy_revision).await
    }

    /// 为成功的无字段变更请求写入审计日志。
    async fn audit_role_update(
        self: &Arc<Self>,
        role: Role,
        audit: PreparedResourceAudit,
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
        audit: PreparedResourceAudit,
        policy_revision: u64,
    ) -> Result<Role> {
        role.update(RoleUpdate { name: Some(name), ..Default::default() })?;
        let db = self.db.clone();
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
            Box::pin(async move {
                db.roles().update(&mut role, session).await?;
                Ok(role)
            })
        })
        .await
    }

    /// 在同一事务中 CAS touch 角色并覆盖权限。
    ///
    /// 角色写触碰会递增版本与更新时间，使权限更新、角色绑定和角色删除在多实例部署下
    /// 对同一角色产生写冲突，避免删除后遗留 policy 引用。
    pub(super) async fn replace_role_permissions(
        self: &Arc<Self>,
        role_id: &str,
        permissions: Vec<Permission>,
        audit: Option<PreparedResourceAudit>,
        expected_revision: Option<u64>,
    ) -> Result<Role> {
        let role_id = role_id.to_string();
        let role_key = role_key(&role_id);
        let permissions = permission_pairs(permissions);
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let audit_port = self.audit.clone();
        self.run_policy_transaction_at_revision(expected_revision, move |session| {
            Box::pin(async move {
                let mut role = role_or_not_found(db.roles().find_by_id(&role_id, session).await?)?;
                db.roles().update(&mut role, session).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, session).await?;
                if let Some(audit) = audit {
                    audit_port.persist(&audit, session).await?;
                }
                Ok(role)
            })
        })
        .await
    }

    /// 使用指定 ID 在同一事务中创建角色实体并写入完整权限规则。
    ///
    /// 仅内建角色初始化可以指定固定 ID；普通角色始终由 [`Self::create_role`] 生成 ID。
    pub(super) async fn create_role_with_id(
        self: &Arc<Self>,
        id: String,
        data: RoleData,
        permissions: Vec<Permission>,
        audit: Option<PreparedResourceAudit>,
        expected_revision: Option<u64>,
    ) -> Result<Role> {
        let role = Role::new(id, data)?;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let audit_port = self.audit.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(permissions);
        self.run_policy_transaction_at_revision(expected_revision, move |session| {
            Box::pin(async move {
                db.roles().create(&role, session).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, session).await?;
                if let Some(audit) = audit {
                    audit_port.persist(&audit, session).await?;
                }
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 读取角色自身的直接权限，不展开继承关系。
    pub(super) async fn direct_role_permissions(&self, role_id: &str) -> Result<PermissionSet> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        permissions_for_role(&enforcer, role_id).map(PermissionSet::new)
    }

    /// 在同一事务中更新角色实体并覆盖完整权限规则。
    async fn update_role_with_permissions(
        self: &Arc<Self>,
        mut role: Role,
        name: String,
        permissions: PermissionSet,
        policy_revision: u64,
        audit: PreparedResourceAudit,
    ) -> Result<Role> {
        role.update(RoleUpdate { name: Some(name), ..Default::default() })?;
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        let permissions = permission_pairs(permissions.into_vec());
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
            Box::pin(async move {
                db.roles().update(&mut role, session).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, session).await?;
                Ok::<Role, Error>(role)
            })
        })
        .await
    }

    /// 原子修复内建 root 角色元数据与完整权限。
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
        self.run_policy_transaction_at_revision(None, move |session| {
            Box::pin(async move {
                if role.base.is_deleted() {
                    db.roles().restore(&mut role, session).await?;
                }
                db.roles().update(&mut role, session).await?;
                policy_store.replace_role_permissions(&role_key, &permissions, session).await?;
                Ok(role)
            })
        })
        .await
    }

    /// 在同一事务中软删除角色实体及其权限和账号绑定。
    async fn delete_role_with_policy(
        self: &Arc<Self>,
        mut role: Role,
        audit: PreparedResourceAudit,
        policy_revision: u64,
    ) -> Result<()> {
        let db = self.db.clone();
        let policy_store = self.policy_store.clone();
        let role_key = role_key(role.base.id.as_str());
        self.run_authorized_audited_policy_transaction(policy_revision, audit, move |session| {
            Box::pin(async move {
                db.roles().soft_delete(&mut role, session).await?;
                policy_store.remove_role(&role_key, session).await?;
                Ok::<(), Error>(())
            })
        })
        .await
    }
}
