//! 所有决定写入的事实使用调用方执行器。
use std::collections::BTreeSet;

use erp_core::AccountKind;
use persistence_core::Executor;

use super::PolicyBundleService;
use crate::entity::authorization_bundle::plan::{PolicyFacts, PolicyRoleState, PolicyUserState};
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, MongoCasbinAdapter, Permission, Result, subject};

impl PolicyBundleService {
    /// 预览、应用或导出时，沿原事务按规范化引用有界读取人员、直接角色绑定、角色和人员范围。
    /// 一并读取人员当前绑定的角色；人员失效、角色形态不受支持或读取失败时返回错误。
    pub(super) async fn facts(
        &self,
        mut roles: BTreeSet<String>,
        user_ids: Vec<String>,
        executor: &mut dyn Executor,
    ) -> Result<PolicyFacts> {
        let mut facts = PolicyFacts::default();

        for user_id in user_ids {
            let user = self.user_fact(&user_id, executor).await?;
            roles.extend(user.role_ids.iter().map(ToString::to_string));
            facts.scopes.extend(
                self.access.db.person_data_scopes().for_person(&user_id, None, None, executor).await?,
            );
            facts.users.insert(user_id, user);
        }

        for id in roles {
            if let Some(role) = self.role_fact(&id, executor).await? {
                facts.roles.insert(id, role);
            }
        }
        Ok(facts)
    }

    /// 收集人员事实时，沿原事务读取有效后台账号的版本及实际 Casbin 直接角色绑定，跳过 `user_roles` 留痕。
    /// 账号不存在或失效、角色键无效或读取失败时返回错误。
    pub(super) async fn user_fact(&self, id: &str, executor: &mut dyn Executor) -> Result<PolicyUserState> {
        let user = self
            .access
            .db
            .accounts()
            .find_by_id(id, executor)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::ValidationError(format!("后台人员不存在或已失效：{id}")))?;

        let keys = MongoCasbinAdapter::new(self.access.db.clone())
            .subject_roles(&subject(AccountKind::Admin, id), executor)
            .await?;
        PolicyUserState::from_role_keys(user.base.version, keys)
    }

    /// 收集角色事实时，沿原事务读取角色（含已删除记录）、直接权限和所有直接绑定的人员标识。
    /// 角色和授权记录均不存在时返回 `None`；孤立授权、继承、非后台主体、影响人数超过 1000 或读取失败时返回错误。
    pub(super) async fn role_fact(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PolicyRoleState>> {
        let store = MongoCasbinAdapter::new(self.access.db.clone());
        let key = format!("role:{id}");
        let subjects = store.role_subjects(&key, executor).await?;
        let parents = store.subject_roles(&key, executor).await?;
        let role = self.access.db.roles().find_by_id_including_deleted(id, executor).await?;
        let permissions = store
            .role_permissions(&key, executor)
            .await?
            .into_iter()
            .map(|(resource, action)| Permission::parse(format!("{resource}:{action}")))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        PolicyRoleState::from_parts(role, permissions, parents, subjects)
    }
}
