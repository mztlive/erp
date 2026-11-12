//! 由当前有效账号角色生成逐操作资格，范围配置不绑定角色。
use application_core::AuditActor;
use persistence_core::{Executor, Transactional};

use super::AccessControlService;
use super::resolve::{AuthorizedDataScope, DataScopeService};
use crate::dto::person_scope::{PersonBusinessOption, PersonScopeView, RetiredPersonScope};
use crate::entity::access_control::scope_spec;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, Permission, Result, RoleRepositoryExt};

impl AccessControlService {
    /// 获取人员唯一范围及服务端有效操作候选。
    /// # 参数
    /// 人员与当前管理员。
    /// # 返回
    /// 业务范围及用于保存的策略版本。
    /// # 错误
    /// 配置读取越权、人员不存在或数据库失败拒绝。
    pub async fn person_scopes(&self, user: &str, actor: &AuditActor) -> Result<PersonScopeView> {
        let service = self.clone();
        let (user, actor) = (user.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let policy_version =
                        service.authorize_person_scope(&actor, false, executor).await?.policy_version;
                    let businesses = service.person_business_options(&user, executor).await?;
                    let items =
                        service.db.person_data_scopes().for_person(&user, None, None, executor).await?;
                    let retired_items = items.iter().filter_map(RetiredPersonScope::from_scope).collect();
                    Ok(PersonScopeView { items, businesses, policy_version, retired_items })
                })
            })
            .await
    }

    /// 公司组织配置边界与配置动作必须由同一有效角色提供。
    /// # 参数
    /// 当前管理员、读写模式及调用方事务执行器。
    /// # 返回
    /// 已证明配置资格的完整上下文，可在本阶段复用其组织事实。
    /// # 错误
    /// 无公司配置资格、策略漂移或持久化失败时拒绝。
    pub(super) async fn authorize_person_scope(
        &self,
        actor: &AuditActor,
        write: bool,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let codes = if write {
            vec!["admin:list", "role:list", "data_scope:create"]
        } else {
            vec!["admin:list", "role:list", "data_scope:list"]
        };
        let permissions =
            codes.into_iter().map(Permission::parse).collect::<std::result::Result<Vec<_>, _>>()?;
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(
                actor,
                &Permission::parse(format!("{}:{}", "org_unit", if write { "manage" } else { "list" }))?,
                &permissions,
                executor,
            )
            .await?;
        if !access.scope.role_clauses.iter().any(|c| c.company)
            || access.scope.user_limit.as_ref().is_some_and(|c| !c.company)
        {
            let message = if write {
                "设置人员范围需要公司范围的组织配置权限"
            } else {
                "查看人员范围需要公司范围的组织查看权限"
            };
            return Err(Error::Forbidden(message.into()));
        }
        Ok(access)
    }

    /// 各操作独立证明，可来自人员不同的有效角色。
    ///
    /// # 参数
    /// * `user` - 人员账号 ID。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回该人员全部已接线业务的有效操作候选；非在职后台账号得到空列表。
    ///
    /// # 错误
    /// 人员不存在、策略漂移或持久化失败时拒绝。
    pub(super) async fn person_business_options(
        &self,
        user: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonBusinessOption>> {
        self.person_business_options_for(user, None, executor).await
    }

    /// 按本次所需业务冻结资格，读取页面时仍返回全部业务候选。
    /// # 参数
    /// 人员、可选业务及原事务执行器。
    /// # 返回
    /// 各动作分别由当前有效角色证明的候选。
    /// # 错误
    /// 人员不存在、策略漂移或持久化失败时拒绝。
    pub(super) async fn person_business_options_for(
        &self,
        user: &str,
        resource: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonBusinessOption>> {
        let account = self
            .db
            .accounts()
            .find_by_id(user, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("人员不存在".into()))?;
        if !account.is_active_backoffice() {
            return Ok(vec![]);
        }
        let resources = match resource {
            Some(resource) if scope_spec::resource_is_known(resource) => vec![resource],
            Some(_) => return Ok(vec![]),
            None => scope_spec::resources(),
        };
        let permissions =
            resources.iter().flat_map(|resource| scope_spec::actions(resource)).cloned().collect::<Vec<_>>();
        let rbac = self.rbac.clone();
        let snapshot = rbac.role_permission_snapshot(account.kind, user, &permissions).await?;
        rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let roles = self.db.roles().enabled_roles(snapshot.role_ids(), executor).await?;
        let mut options = Vec::new();
        for resource in resources {
            let mut granted: Vec<String> = Vec::new();
            for permission in scope_spec::actions(resource) {
                let ids = snapshot.granting_role_ids(permission);
                if roles.iter().any(|role| ids.iter().any(|id| id.as_str() == role.base.id)) {
                    granted.push(permission.action().into());
                }
            }
            if !granted.is_empty() {
                options.push(PersonBusinessOption::from_granted(resource, granted)?);
            }
        }
        Ok(options)
    }
}
