//! 所有决定写入的事实使用调用方执行器。
use std::collections::BTreeSet;

use erp_core::AccountKind;
use persistence_core::Executor;

use super::PolicyBundleService;
use crate::entity::authorization_bundle::PolicyDocument;
use crate::entity::authorization_bundle::plan::{PolicyFacts, PolicyRoleState, PolicyUserState};
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, MongoCasbinAdapter, Permission, Result, subject};

impl PolicyBundleService {
    /// 按文件引用有界读取人员、直接绑定、角色及范围。
    /// # 参数
    /// document 为规范化文件，executor 为原事务执行器。
    /// # 返回
    /// 同一事务内读取的目标事实。
    /// # 错误
    /// 人员失效、角色形态不受支持或读取失败时拒绝。
    pub(super) async fn facts(
        &self,
        document: &PolicyDocument,
        executor: &mut dyn Executor,
    ) -> Result<PolicyFacts> {
        let mut facts = PolicyFacts::default();
        let mut roles = document.roles.iter().map(|role| role.id.to_string()).collect::<BTreeSet<_>>();
        roles.extend(document.bindings.iter().flat_map(|b| b.role_ids.iter().map(ToString::to_string)));
        for user_id in document.user_ids() {
            let user = self.user_fact(&user_id, executor).await?;
            roles.extend(user.role_ids.iter().cloned());
            facts.users.insert(user_id.clone(), user);
            facts.scopes.extend(
                self.access.db.person_data_scopes().for_person(&user_id, None, None, executor).await?,
            );
        }
        for id in roles {
            if let Some(role) = self.role_fact(&id, executor).await? {
                facts.roles.insert(id, role);
            }
        }
        Ok(facts)
    }

    /// 用户必须为当前有效后台账号；不读取 user_roles 留痕替代真实绑定。
    /// # 参数
    /// id 为后台账号标识，executor 为原事务执行器。
    /// # 返回
    /// 账号版本及真实角色绑定。
    /// # 错误
    /// 账号失效、角色键无效或读取失败时拒绝。
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

    /// 角色权限和所有直接受影响人员均从持久化事实读取。
    /// # 参数
    /// id 为角色标识，executor 为原事务执行器。
    /// # 返回
    /// 现有角色及直接权限和影响人员；全新身份返回空。
    /// # 错误
    /// 存在孤立授权、继承、非后台主体、影响超限或读取失败时拒绝。
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
