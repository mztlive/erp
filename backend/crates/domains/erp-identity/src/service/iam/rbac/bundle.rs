//! 统一授权文件复用原 policy 写入和审计，不打开嵌套事务。
use application_core::AuditActor;
use erp_core::AccountKind;
use persistence_core::Executor;

use super::RbacService;
use super::policy::{implicit_permissions_for_role, permission_pairs, role_key};
use crate::entity::authorization_bundle::plan::{PlannedBinding, PlannedRole};
use crate::{
    AccessControlExt, PermissionSet, Result, Role, RoleData, RoleIdSet, RoleRepositoryExt, RoleUpdate,
};

impl RbacService {
    /// 预览或重验授权文件时，读取操作人的 Casbin 绑定，在原事务中筛出启用角色，以其隐式权限并集作为授予上限。
    /// 策略快照、角色读取或权限解析失败时返回错误；调用方须核对事务可见的策略版本。
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

    /// 应用重验后的单个角色计划，在原事务中创建角色或更新名称、替换整套直接权限并保存资源审计。
    /// 角色校验、持久化或审计失败时将错误返回外层事务。
    pub(crate) async fn write_bundle_role(
        &self,
        planned: PlannedRole,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let action = if planned.existing.is_some() { "role.update" } else { "role.create" };
        let mut role = match planned.existing {
            Some(mut role) => {
                role.update(RoleUpdate { name: Some(planned.name), ..Default::default() })?;
                self.db.roles().update(&mut role, executor).await?;
                role
            },
            None => {
                let role = Role::new(planned.id, RoleData::new(planned.name))?;
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

    /// 应用重验后的完整绑定计划，在原事务中更新角色版本、覆盖后台账号的 Casbin 绑定、初始化目录资格并写审计。
    /// 角色校验、绑定、资格初始化或审计失败时将错误返回外层事务。
    pub(crate) async fn write_bundle_binding(
        &self,
        planned: PlannedBinding,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.write_subject_roles(
            AccountKind::Admin,
            &planned.user_id,
            RoleIdSet::from(planned.role_ids),
            executor,
        )
        .await?;

        let audit = self.audit.resource_log(actor.clone(), "admin.role.update", "admin", planned.user_id)?;
        self.audit.persist(&audit, executor).await
    }
}
