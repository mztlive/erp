//! 由当前有效账号角色生成逐操作资格，范围配置不绑定角色。
use application_core::AuditActor;
use erp_permissions::access_control;
use persistence_core::{Executor, Transactional};

use super::AccessControlService;
use crate::dto::person_scope::{PersonBusinessOption, PersonScopeView, RetiredPersonScope};
use crate::entity::access_control::scope_spec;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, AccessTarget, Error, Permission, Result};

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
                    let policy_version = service.authorize_person_scope(&actor, false, executor).await?;
                    // 沿用配置读取的组织快照检查，即使响应仅使用策略版本。
                    OrganizationRepository::new(&service.db).state(executor).await?;
                    let granted = service.person_granted_actions(&user, None, executor).await?;
                    let businesses = granted
                        .chunk_by(|left, right| left.resource() == right.resource())
                        .map(|actions| {
                            PersonBusinessOption::from_granted(
                                actions[0].resource(),
                                actions.iter().map(|permission| permission.action().into()).collect(),
                            )
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let items =
                        service.db.person_data_scopes().for_person(&user, None, None, executor).await?;
                    let retired_items = items.iter().filter_map(RetiredPersonScope::from_scope).collect();
                    Ok(PersonScopeView { items, businesses, policy_version, retired_items })
                })
            })
            .await
    }

    /// 在原事务检查人员范围入口的公司级配置资格，并返回当前策略版本。
    pub(super) async fn authorize_person_scope(
        &self,
        actor: &AuditActor,
        write: bool,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        let entry =
            if write { &access_control::PERSON_SCOPE_SAVE } else { &access_control::PERSON_SCOPE_LIST };
        self.authorization.require(actor, entry, AccessTarget::Operation, executor).await?;
        self.rbac.policy_revision_with_executor(executor).await
    }

    /// 沿原事务读取启用角色授予的公共动作，按目录顺序返回。
    pub(super) async fn person_granted_actions(
        &self,
        user: &str,
        resource: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<Permission>> {
        let account = self
            .db
            .accounts()
            .find_by_id(user, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("人员不存在".into()))?;
        if !account.is_active_backoffice() {
            return Ok(vec![]);
        }

        let mut permissions = match resource {
            Some(resource) if scope_spec::resource_is_known(resource) => {
                scope_spec::display_actions(resource).cloned().collect::<Vec<_>>()
            },
            Some(_) => return Ok(vec![]),
            None => {
                scope_spec::resources().into_iter().flat_map(scope_spec::display_actions).cloned().collect()
            },
        };
        let target = AuditActor::new(user.into(), account.secret.account().into(), account.kind);
        let granted = self
            .rbac
            .qualified_permissions_for_account(&target, &account, &permissions, executor)
            .await?
            .permissions();
        permissions.retain(|permission| granted.covers_one(permission));
        Ok(permissions)
    }
}
