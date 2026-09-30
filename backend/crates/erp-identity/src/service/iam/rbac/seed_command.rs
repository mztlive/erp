//! 显式初始化命令使用的角色种子；沿用操作人授权、策略版本和审计事务。

use std::sync::Arc;

use application_core::AuditActor;
use persistence_core::NoTransaction;

use super::{ROOT_ROLE_ID, RbacService};
use crate::entity::role::RoleData;
use crate::{AccessControlExt, CreateRoleParams, Error, Result, UpdateRoleParams};

impl RbacService {
    /// 以发起人的权限创建固定种子角色，或补齐已有同身份角色的缺失权限。
    /// # 参数
    /// `id` 为调用方固定种子键；`params` 为名称和权限；`actor` 为命令发起人。
    /// # 返回
    /// 创建或补齐返回 true，已满足声明返回 false。
    /// # 错误
    /// 越权、受保护或失效角色、身份冲突、策略版本变化及持久化失败时拒绝。
    pub async fn ensure_seeded_role(
        self: &Arc<Self>,
        id: &str,
        params: CreateRoleParams,
        actor: AuditActor,
    ) -> Result<bool> {
        if id == ROOT_ROLE_ID {
            return Err(Error::Forbidden("超级管理员角色不能通过种子命令配置".into()));
        }
        let authorized = self.authorize_permissions(&actor, params.permissions).await?;
        let existing = self.db.roles().find_by_id_including_deleted(id, &mut NoTransaction).await?;
        if let Some(role) = existing {
            let current = self.direct_role_permissions(id).await?;
            let Some(permissions) =
                role.seeded_permissions(&params.name, &current, &authorized.permissions)?
            else {
                return Ok(false);
            };
            self.update_role(
                id,
                UpdateRoleParams { name: None, permissions: Some(permissions.into_vec()) },
                actor,
            )
            .await?;
        } else {
            let audit = self.audit.resource_log(actor, "role.create", "role", id.to_owned())?;
            self.create_role_with_id(
                id.to_owned(),
                RoleData::new(params.name),
                authorized.permissions.into_vec(),
                Some(audit),
                Some(authorized.policy_revision),
            )
            .await?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Permission;
    use crate::entity::rbac::PermissionSet;
    use crate::entity::role::Role;

    /// 使用正式合并规则覆盖旧销售角色缺少新目录权限的场景及重复执行。
    #[test]
    fn seeded_permissions_fill_missing_directory_without_removing_existing_grants() {
        let role = Role::new("role-demo-sales".into(), RoleData::new("演示销售")).unwrap();
        let current = PermissionSet::new([Permission::parse("sales_order:*").unwrap()]);
        let desired = PermissionSet::new([
            Permission::parse("sales_order:create").unwrap(),
            Permission::parse("sales_person:list").unwrap(),
        ]);
        let merged = role.seeded_permissions("演示销售", &current, &desired).unwrap().unwrap();
        assert!(merged.covers(&current));
        assert!(merged.covers(&desired));
        assert_eq!(merged.as_slice().len(), 2);
        assert!(role.seeded_permissions("演示销售", &merged, &desired).unwrap().is_none());
    }

    /// 删除、停用、系统角色和身份冲突均不能被种子悄悄恢复。
    #[test]
    fn seeded_permissions_reject_conflicting_and_inactive_roles() {
        let current = PermissionSet::default();
        let mut role = Role::new("role-demo-sales".into(), RoleData::new("演示销售")).unwrap();
        assert!(role.seeded_permissions("其他角色", &current, &current).is_err());
        role.disabled = true;
        assert!(role.seeded_permissions("演示销售", &current, &current).is_err());
        role.disabled = false;
        role.system = true;
        assert!(role.seeded_permissions("演示销售", &current, &current).is_err());
        role.system = false;
        role.base.deleted_at = 1;
        assert!(role.seeded_permissions("演示销售", &current, &current).is_err());
        role.base.deleted_at = 0;
        role.base.id = ROOT_ROLE_ID.into();
        assert!(role.seeded_permissions("演示销售", &current, &current).is_err());
    }
}
