use std::collections::HashMap;

use application_core::AuditActor;
use casbin::RbacApi;
use erp_core::AccountKind;
use persistence_core::NoTransaction;

use super::authorize::role_is_assignable;
use super::policy::{
    collect_role_ids, collect_role_permissions, implicit_permissions_for_role, parse_policy_permissions,
    permissions_for_actor, role_ids_for_account,
};
use super::{RbacService, subject};
use crate::AccessControlExt;
use crate::dto::RoleItem;
use crate::entity::rbac::Permission;
use crate::entity::role::Role;
use crate::error::Result;

/// 由批量直接权限装配角色响应项（两个列表入口共用该映射）。
///
/// # 参数
/// * `roles` - 待装配的角色
/// * `permissions` - 按角色 ID 索引的直接权限；命中项会被取出
///
/// # 返回
/// 与输入角色一一对应的响应项；缺权限的角色携带空权限。
fn role_items_from_map(
    roles: Vec<Role>,
    permissions: &mut HashMap<String, Vec<Permission>>,
) -> Vec<RoleItem> {
    roles
        .into_iter()
        .map(|role| {
            let role_permissions = permissions.remove(&role.base.id).unwrap_or_default();
            RoleItem::from_role(role, role_permissions)
        })
        .collect()
}

impl RbacService {
    /// 查询全部角色及其直接权限。
    ///
    /// # 错误
    /// 当 MongoDB 或 Casbin policy 查询失败时返回错误。
    pub async fn role_list(&self) -> Result<Vec<RoleItem>> {
        let roles = self.db.roles().list_all(&mut NoTransaction).await?;
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
            .list_enabled(&mut NoTransaction)
            .await?
            .into_iter()
            .filter(role_is_assignable)
            .collect::<Vec<_>>();
        self.assignable_role_items(roles, actor).await
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
        let mut manageable = Vec::with_capacity(roles.len());
        for role in roles {
            let required_permissions = implicit_permissions_for_role(&enforcer, role.base.id.as_str())?;
            if actor_permissions.covers(&required_permissions) {
                manageable.push(role);
            }
        }
        Ok(role_items_from_map(manageable, &mut permissions))
    }

    /// 为一批角色装配直接权限；整个批次只读取一次 Enforcer。
    async fn role_items(&self, roles: Vec<Role>) -> Result<Vec<RoleItem>> {
        if roles.is_empty() {
            return Ok(Vec::new());
        }

        let role_ids = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
        let enforcer = self.fresh_enforcer().await?.read().await;
        let mut permissions = collect_role_permissions(&enforcer, &role_ids)?;

        Ok(role_items_from_map(roles, &mut permissions))
    }

    /// 查询账号直接绑定的角色 ID。
    ///
    /// # 错误
    /// 当 Casbin policy 加载失败时返回错误。
    pub async fn role_ids(&self, account_kind: AccountKind, account_id: &str) -> Result<Vec<String>> {
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
    pub async fn role_ids_by_accounts(
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
    pub async fn permissions(&self, account_kind: AccountKind, account_id: &str) -> Result<Vec<Permission>> {
        let policies = self
            .fresh_enforcer()
            .await?
            .read()
            .await
            .get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
        parse_policy_permissions(policies)
    }
}
