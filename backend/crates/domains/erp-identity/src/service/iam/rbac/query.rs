use std::collections::HashMap;
use std::result::Result as StdResult;

use application_core::AuditActor;
use casbin::RbacApi;
use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};

use super::policy::{
    collect_role_ids, collect_role_permissions, implicit_permissions_for_role, parse_policy_permissions,
    permissions_for_actor, role_ids_for_account, role_key,
};
use super::{RbacService, subject};
use crate::dto::RoleItem;
use crate::entity::rbac::{PermissionSet, RoleId};
use crate::entity::role::Role;
use crate::error::Result;
use crate::repository::prelude::*;
use crate::{AccessControlExt, MongoCasbinAdapter};

/// 由批量直接权限装配角色响应项（两个列表入口共用该映射）。
///
/// # 参数
/// * `roles` - ID 不重复的待装配角色
/// * `permissions` - 包含全部待装配角色 ID 的直接权限；命中项会被取出
///
/// # 返回
/// 与输入角色一一对应的响应项；没有权限的角色使用映射中已有的空权限集合。
///
/// # Panics
/// 角色 ID 重复或权限映射缺少待装配角色时触发，表示调用方破坏了批量装配不变式。
fn role_items_from_map(roles: Vec<Role>, permissions: &mut HashMap<String, PermissionSet>) -> Vec<RoleItem> {
    roles
        .into_iter()
        .map(|role| {
            let role_permissions =
                permissions.remove(&role.base.id).expect("权限映射由全部角色 ID 构造，且每个角色仅装配一次");
            RoleItem::from_role(role, role_permissions.into_vec())
        })
        .collect()
}

impl RbacService {
    /// 在调用方事务中读取仍启用的直接账号角色。
    /// # 参数
    /// account_kind、account_id 为账号身份，executor 为原事务执行器。
    /// # 返回
    /// 返回排序去重且仍启用的角色 ID。
    /// # 错误
    /// 角色身份非法或数据库读取失败时返回错误。
    pub async fn role_ids_with_executor(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<RoleId>> {
        let keys = MongoCasbinAdapter::new(self.db.clone())
            .subject_roles(&subject(account_kind, account_id), executor)
            .await?;
        let mut ids = keys
            .iter()
            .filter_map(|key| key.strip_prefix("role:"))
            .map(RoleId::parse)
            .collect::<StdResult<Vec<_>, _>>()?;
        ids.sort();
        ids.dedup();
        let enabled = self.enabled_role_ids(&ids, executor).await?;
        ids.retain(|id| enabled.contains(id));
        Ok(ids)
    }

    /// 保留调用方快照中仍启用的角色。
    /// # 参数
    /// ids 为候选角色，executor 沿用调用方事务。
    /// # 返回
    /// 返回当前启用的类型化角色 ID；空候选不查询数据库。
    /// # 错误
    /// 角色数据非法或数据库读取失败时返回错误。
    pub async fn enabled_role_ids(&self, ids: &[RoleId], executor: &mut dyn Executor) -> Result<Vec<RoleId>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .roles()
            .enabled_roles(ids, executor)
            .await?
            .into_iter()
            .map(|role| RoleId::parse(role.base.id))
            .collect::<StdResult<Vec<_>, _>>()?)
    }

    /// 查询全部角色及其直接权限。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回带直接权限的角色列表；没有角色时为空列表。
    ///
    /// # 错误
    /// 当 MongoDB 或 Casbin policy 查询失败时返回错误。
    pub async fn role_list(&self) -> Result<Vec<RoleItem>> {
        let roles = self.db.roles().list_all(&mut NoTransaction).await?;
        self.role_items(roles).await
    }

    /// 查询全部可分配角色及其直接权限。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人，用于按自身权限过滤。
    ///
    /// # 返回
    /// 返回操作人权限能够覆盖、且允许普通分配的角色。
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
            .filter(|role| role.ensure_assignable().is_ok())
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
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    ///
    /// # 返回
    /// 返回去掉 `role:` 前缀的直接角色 ID；未绑定时为空列表。
    ///
    /// # 错误
    /// 当 Casbin policy 加载失败时返回错误。
    pub async fn role_ids(&self, account_kind: AccountKind, account_id: &str) -> Result<Vec<RoleId>> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        role_ids_for_account(&enforcer, account_kind, account_id)
    }

    /// 读取直接绑定某稳定角色的后台账号 ID。
    ///
    /// # 参数
    /// * `role_id` - 稳定角色 ID，不含 `role:` 前缀
    ///
    /// # 返回
    /// 返回排序后的后台账号 ID。非后台主体被忽略。
    ///
    /// # 错误
    /// 策略加载失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只反映当前 Casbin 绑定，不把角色撤销解释成查询资格终止。
    pub async fn direct_admin_ids_for_role(&self, role_id: &str) -> Result<Vec<String>> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        let mut ids = enforcer
            .get_users_for_role(&role_key(role_id), None)
            .into_iter()
            .filter_map(|subject| {
                subject.strip_prefix("user:admin:").filter(|id| !id.is_empty()).map(str::to_string)
            })
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// 批量查询同类账号直接绑定的角色 ID。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_ids` - 待查询的账号 ID 集合。
    ///
    /// # 返回
    /// 返回账号 ID 到角色 ID 集合的映射；未绑定角色的账号对应空集合。
    ///
    /// # 错误
    /// 当 Casbin policy 加载失败时返回错误。
    pub async fn role_ids_by_accounts(
        &self,
        account_kind: AccountKind,
        account_ids: &[String],
    ) -> Result<HashMap<String, Vec<RoleId>>> {
        if account_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let enforcer = self.fresh_enforcer().await?.read().await;
        collect_role_ids(&enforcer, account_kind, account_ids)
    }

    /// 查询账号通过角色继承获得的权限。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    ///
    /// # 返回
    /// 返回去重后的隐式权限。
    ///
    /// # 错误
    /// Casbin policy 无法加载，或策略行无法解析为权限时返回错误。
    pub async fn permissions(&self, account_kind: AccountKind, account_id: &str) -> Result<PermissionSet> {
        let policies = self
            .fresh_enforcer()
            .await?
            .read()
            .await
            .get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
        parse_policy_permissions(policies)
    }
}

#[cfg(test)]
mod tests {
    use casbin::{CoreApi, DefaultModel, Enforcer, MemoryAdapter};

    use super::*;
    use crate::service::iam::rbac::RBAC_MODEL;
    use crate::{Permission, RoleData};

    /// 实际批量读取结果可装配完整列表或筛选子集，空权限角色不会缺失。
    #[tokio::test]
    async fn role_items_preserve_order_and_empty_permissions() {
        let model = DefaultModel::from_str(RBAC_MODEL).await.unwrap();
        let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
        enforcer
            .add_permission_for_user(&role_key("role-a"), vec!["customer".into(), "list".into()])
            .await
            .unwrap();
        let roles = ["role-b", "role-a"]
            .into_iter()
            .map(|id| Role::new(RoleId::parse(id).unwrap(), RoleData::new(id)).unwrap())
            .collect::<Vec<_>>();
        let ids = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
        let mut permissions = collect_role_permissions(&enforcer, &ids).unwrap();
        let items = role_items_from_map(roles.clone(), &mut permissions);
        assert_eq!(items[0].id, "role-b");
        assert!(items[0].permissions.is_empty());
        assert_eq!(items[1].id, "role-a");
        assert_eq!(items[1].permissions, vec![Permission::parse("customer:list").unwrap()]);

        let mut permissions = collect_role_permissions(&enforcer, &ids).unwrap();
        let items = role_items_from_map(vec![roles[0].clone()], &mut permissions);
        assert_eq!(items.len(), 1);
        assert!(items[0].permissions.is_empty());
    }
}
