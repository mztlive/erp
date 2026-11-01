//! 统一授权文件复用原 policy 写入和审计，不打开嵌套事务。
use application_core::AuditActor;
use erp_core::AccountKind;
use persistence_core::Executor;

use super::RbacService;
use super::policy::{implicit_permissions_for_role, permission_pairs, role_key};
use crate::entity::authorization_bundle::plan::{PlannedBinding, PlannedRole};
use crate::{AccessControlExt, PermissionSet, Result, Role, RoleData, RoleRepositoryExt, RoleUpdate};

impl RbacService {
    /// 从当前启用角色构造授予上限，停用角色不能提供上限。
    /// # 参数
    /// actor 为认证身份，executor 为原事务执行器。
    /// # 返回
    /// 当前有效角色的隐式权限集合。
    /// # 错误
    /// 策略快照、角色读取或权限解析失败时拒绝。
    pub(crate) async fn bundle_actor_permissions(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PermissionSet> {
        let ids = self.role_ids(actor.kind(), actor.id()).await?;
        let roles = self.db.roles().enabled_roles(&ids, executor).await?;
        let enforcer = self.fresh_enforcer().await?.read().await;
        let mut permissions = vec![];
        for role in roles {
            permissions.extend(implicit_permissions_for_role(&enforcer, &role.base.id)?.into_vec());
        }
        Ok(PermissionSet::new(permissions))
    }

    /// 写入已重验的单个角色及策略和原有审计。
    /// # 参数
    /// planned 为重验后的角色计划，actor 为操作人，executor 为原授权事务。
    /// # 返回
    /// 当前步骤写入成功。
    /// # 错误
    /// 角色校验、仓储或审计失败时向原事务返回错误。
    pub(crate) async fn write_bundle_role(
        &self,
        planned: PlannedRole,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let action = if planned.existing.is_some() { "role.update" } else { "role.create" };
        let mut role = match planned.existing {
            Some(mut role) => {
                role.update(RoleUpdate { name: Some(planned.declaration.name), ..Default::default() })?;
                self.db.roles().update(&mut role, executor).await?;
                role
            },
            None => {
                let role =
                    Role::new(planned.declaration.id.to_string(), RoleData::new(planned.declaration.name))?;
                self.db.roles().create(&role, executor).await?;
                role
            },
        };
        self.policy_store
            .replace_role_permissions(
                &role_key(&role.base.id),
                &permission_pairs(planned.permissions),
                executor,
            )
            .await?;
        let audit =
            self.audit.resource_log(actor.clone(), action, "role", std::mem::take(&mut role.base.id))?;
        self.audit.persist(&audit, executor).await
    }

    /// 沿用账号角色分配、角色版本触碰和目录资格初始化。
    /// # 参数
    /// planned 为重验后的完整角色集合，actor 为操作人，executor 为原授权事务。
    /// # 返回
    /// 当前绑定步骤及审计写入成功。
    /// # 错误
    /// 绑定、资格初始化或审计失败时向原事务返回错误。
    pub(crate) async fn write_bundle_binding(
        &self,
        planned: PlannedBinding,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.write_subject_roles(AccountKind::Admin, &planned.user_id, planned.role_ids, executor).await?;
        let audit = self.audit.resource_log(actor.clone(), "admin.role.update", "admin", planned.user_id)?;
        self.audit.persist(&audit, executor).await
    }
}
