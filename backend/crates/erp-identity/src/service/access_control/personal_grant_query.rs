//! 人员当前持有角色和已接线业务动作，以服务端实时证据生成选择项。
use persistence_core::Executor;

use super::AccessControlService;
use super::consumers::department_resources;
use crate::dto::personal_grant::{
    GrantBusinessOption, GrantRoleOption, PersonalGrantListView, PersonalGrantView,
};
use crate::repository::RoleRepositoryExt;
use crate::repository::access_control::personal_grant::PersonalBusinessGrantRepositoryExt;
use crate::{AccessControlExt, Permission, Result};

impl AccessControlService {
    /// 在读取事务内生成有效动作及角色依据，不把失效授权展示为生效。
    /// # 参数
    /// 指定人员、配置策略版本与读取执行器。
    /// # 返回
    /// 带有效动作、依据角色及业务选择项的配置视图。
    /// # 错误
    /// 持久化或策略快照漂移时返回错误。
    pub(super) async fn personal_grant_view(
        &self,
        user_id: &str,
        policy_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<PersonalGrantListView> {
        let roles = self.personal_grant_options(user_id, executor).await?;
        let items = self.db.personal_business_grants().for_person(user_id, None, executor).await?;
        let items = items
            .into_iter()
            .map(|grant| {
                let active_actions = roles
                    .iter()
                    .find(|role| role.id == grant.role_id)
                    .and_then(|role| role.resources.iter().find(|entry| entry.resource == grant.resource))
                    .map(|entry| {
                        grant
                            .actions
                            .iter()
                            .filter(|action| entry.actions.contains(action))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                PersonalGrantView { grant, active_actions }
            })
            .collect();
        Ok(PersonalGrantListView { items, roles, policy_version })
    }

    /// 只返回当前启用账号、启用角色实际提供的部门业务动作。
    async fn personal_grant_options(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<GrantRoleOption>> {
        let Some(account) = self
            .db
            .accounts()
            .find_by_id(user_id, executor)
            .await?
            .filter(|account| account.is_active_backoffice())
        else {
            return Ok(vec![]);
        };
        let permissions = department_resources()
            .flat_map(|(resource, actions)| {
                actions.iter().map(move |action| Permission::parse(format!("{resource}:{action}")))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let rbac = self.grant_rbac()?;
        let snapshot = rbac.role_permission_snapshot(account.kind, user_id, &permissions).await?;
        rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let roles = self.db.roles().enabled_roles(snapshot.role_ids(), executor).await?;
        let mut options = Vec::new();
        for role in roles {
            let mut resources = Vec::new();
            for (resource, actions) in department_resources() {
                let mut granted = Vec::new();
                for action in actions {
                    let permission = Permission::parse(format!("{resource}:{action}"))?;
                    if snapshot.granting_role_ids(&permission).contains(&role.base.id) {
                        granted.push((*action).to_string());
                    }
                }
                if !granted.is_empty() {
                    resources.push(GrantBusinessOption { resource: resource.into(), actions: granted });
                }
            }
            if !resources.is_empty() {
                options.push(GrantRoleOption { id: role.base.id, name: role.name, resources });
            }
        }
        Ok(options)
    }
}
