//! 由当前有效账号角色生成逐操作资格，范围配置不绑定角色。
use application_core::AuditActor;
use erp_permissions::access_control;
use persistence_core::{Executor, Transactional};

use super::AccessControlService;
use crate::dto::person_scope::{PersonBusinessOption, PersonScopeView, RetiredPersonScope};
use crate::entity::access_control::scope_spec;
use crate::entity::organization_change::OrganizationState;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, AccessTarget, Error, Result, RoleRepositoryExt};

/// 人员范围入口授权通过后，从当前事务取得的事实，供配置读取和写入校验复用。
pub(super) struct PersonScopeFacts {
    /// 当前策略修订号，供配置展示及后续保存做版本比对。
    pub(super) policy_version: u64,
    /// 同一事务读到的组织状态，供显式部门目标校验。
    pub(super) organizations: OrganizationState,
}

impl AccessControlService {
    /// 管理员打开人员范围配置页时，同一事务返回既有范围（含退役项）、当前操作候选和保存用的策略版本。
    /// 管理员无读取资格、目标人员不存在或数据库读取失败时拒绝。
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

    /// 读取或保存人员范围前，由同一有效角色满足公司配置边界和对应读写权限，再用原事务读取策略版本和组织事实。
    /// 缺少配置资格、策略漂移或持久化读取失败时拒绝。
    pub(super) async fn authorize_person_scope(
        &self,
        actor: &AuditActor,
        write: bool,
        executor: &mut dyn Executor,
    ) -> Result<PersonScopeFacts> {
        let entry =
            if write { &access_control::PERSON_SCOPE_SAVE } else { &access_control::PERSON_SCOPE_LIST };
        self.authorization.require(actor, entry, AccessTarget::Operation, executor).await?;
        Ok(PersonScopeFacts {
            policy_version: self.rbac.policy_revision_with_executor(executor).await?,
            organizations: OrganizationRepository::new(&self.db).state(executor).await?,
        })
    }

    /// 打开配置页时，沿当前事务列出全部已登记业务的操作候选；各动作可由不同启用角色授予。
    /// 非有效后台账号返回空列表；人员不存在、策略漂移或读取失败时拒绝。
    pub(super) async fn person_business_options(
        &self,
        user: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonBusinessOption>> {
        self.person_business_options_for(user, None, executor).await
    }

    /// 展示配置或重验保存资格时，按指定资源（未指定则按全部登记资源）证明各动作由当前启用角色授予。
    /// 资格快照须与原事务策略一致；人员不存在、策略漂移或读取失败时拒绝。
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
        let permissions = resources
            .iter()
            .flat_map(|resource| scope_spec::display_actions(resource))
            .cloned()
            .collect::<Vec<_>>();

        let rbac = self.rbac.clone();
        let snapshot = rbac.role_permission_snapshot(account.kind, user, &permissions).await?;
        rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let roles = self.db.roles().enabled_roles(snapshot.role_ids(), executor).await?;

        let mut options = Vec::new();
        for resource in resources {
            let mut granted: Vec<String> = Vec::new();
            for permission in scope_spec::display_actions(resource) {
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
