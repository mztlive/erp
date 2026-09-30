//! 由当前有效账号角色生成逐操作资格，范围配置不绑定角色。
use application_core::AuditActor;
use persistence_core::{Executor, Transactional};

use super::AccessControlService;
use super::consumers::WIRED_CONSUMERS;
use super::resolve::DataScopeService;
use crate::dto::person_scope::{PersonBusinessOption, PersonScopeView, RetiredPersonScope};
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, Permission, Result, RoleRepositoryExt, SharedRbacService};

impl AccessControlService {
    /// 获取人员唯一范围及服务端有效操作候选。
    /// # 参数
    /// 人员与当前管理员。
    /// # 返回
    /// 业务范围及用于保存的策略版本。
    /// # 错误
    /// 配置读取越权、人员不存在或数据库失败拒绝。
    pub async fn person_scopes(&self, user: &str, actor: &AuditActor) -> Result<PersonScopeView> {
        let service = self.scope_service();
        let (user, actor) = (user.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let policy_version = service.authorize_person_scope(&actor, false, executor).await?;
                    let businesses = service.person_business_options(&user, executor).await?;
                    let items =
                        service.db.person_data_scopes().for_person(&user, None, None, executor).await?;
                    let retired_items = items.iter().filter_map(RetiredPersonScope::from_scope).collect();
                    Ok(PersonScopeView { items, businesses, policy_version, retired_items })
                })
            })
            .await
    }

    pub(super) fn scope_service(&self) -> Self {
        Self { db: self.db.clone(), rbac: self.rbac.clone(), targets: self.targets.clone() }
    }
    pub(super) fn scope_rbac(&self) -> Result<SharedRbacService> {
        self.rbac.clone().ok_or_else(|| Error::Forbidden("未装配人员范围服务".into()))
    }

    /// 公司组织配置边界与配置动作必须由同一有效角色提供。
    pub(super) async fn authorize_person_scope(
        &self,
        actor: &AuditActor,
        write: bool,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        let codes = if write {
            vec!["admin:list", "role:list", "data_scope:create"]
        } else {
            vec!["admin:list", "role:list", "data_scope:list"]
        };
        let permissions =
            codes.into_iter().map(Permission::parse).collect::<std::result::Result<Vec<_>, _>>()?;
        let access = DataScopeService::new(self.db.clone(), self.scope_rbac()?)
            .resolve_permissions(
                actor,
                "org_unit",
                if write { "manage" } else { "list" },
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
        Ok(access.policy_version)
    }

    /// 各操作独立证明，可来自人员不同的有效角色。
    pub(super) async fn person_business_options(
        &self,
        user: &str,
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
        let permissions = WIRED_CONSUMERS
            .iter()
            .flat_map(|(r, actions, _)| actions.iter().map(move |a| Permission::parse(format!("{r}:{a}"))))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let rbac = self.scope_rbac()?;
        let snapshot = rbac.role_permission_snapshot(account.kind, user, &permissions).await?;
        rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let roles = self.db.roles().enabled_roles(snapshot.role_ids(), executor).await?;
        let mut options = Vec::new();
        for (resource, actions, dimensions) in WIRED_CONSUMERS {
            let mut granted: Vec<String> = Vec::new();
            for action in *actions {
                let ids = snapshot.granting_role_ids(&Permission::parse(format!("{resource}:{action}"))?);
                if roles.iter().any(|r| ids.contains(&r.base.id)) {
                    granted.push((*action).into());
                }
            }
            if !granted.is_empty() {
                options.push(PersonBusinessOption::from_granted(resource, granted, dimensions)?);
            }
        }
        Ok(options)
    }
}
