//! 原子保存所选操作的附加授权，审计与策略版本同事务提交。

use application_core::AuditActor;
use entity_core::BaseModel;
use persistence_core::Executor;

use super::AccessControlService;
use crate::access_control::ScopeDimension;
use crate::dto::person_scope::PersonScopeCommand;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::access_control::scope_spec;
use crate::entity::access_control::scope_target::ScopeTarget;
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, Permission, Result};

impl AccessControlService {
    /// 管理员保存人员范围时，完整替换所选业务动作的附加授权，不改变角色授予的操作权限。
    /// 范围、审计及策略版本原子提交；越权、版本过期、目标无效或人员缺少动作资格时拒绝整次保存。
    pub async fn save_person_scope(
        &self,
        user: &str,
        req: PersonScopeCommand,
        actor: &AuditActor,
    ) -> Result<()> {
        let event = self
            .build_audit_event(
                actor,
                "person_data_scope.replace",
                "person_data_scope",
                Some(user.into()),
                vec!["resource".into(), "actions".into(), "expression".into()],
            )
            .await?;

        let service = self.clone();
        let (user, actor) = (user.to_owned(), actor.clone());
        self.rbac
            .clone()
            .run_authorized_policy_transaction(req.expected_policy_version, move |executor| {
                Box::pin(async move {
                    service.authorize_person_scope(&actor, true, executor).await?;
                    let organizations = OrganizationRepository::new(&service.db).state(executor).await?;
                    let granted =
                        service.person_granted_actions(&user, Some(&req.resource), executor).await?;
                    ensure_granted_actions(&granted, &req.actions)?;
                    service.validate_person_scope_targets(req.targets(), &organizations, executor).await?;

                    service.replace_person_scope(&user, &req, executor).await?;
                    service.db.audit_events().create(&event, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    /// 保存或审核配置前，沿原事务按输入顺序核对规范化命令引用的目标是否存在且适用于指定维度。
    /// 首次遇到明确部门目标才构建组织树；部门失效、维度不符、外域校验未装配或读取失败时拒绝。
    pub(super) async fn validate_person_scope_targets(
        &self,
        targets: &[ScopeTarget],
        organizations: &OrganizationState,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut tree = None;
        for target in targets {
            match target {
                ScopeTarget::OrgExplicit { ids, .. } => {
                    if tree.is_none() {
                        tree = Some(OrgTree::new(&organizations.units)?);
                    }
                    for id in ids {
                        if tree.as_ref().expect("首次显式组织目标已加载组织树").expand(id, false)?.is_empty()
                        {
                            return Err(Error::ValidationError("目标部门已停用".into()));
                        }
                    }
                },
                ScopeTarget::SettlementParties(ids) => {
                    self.targets.validate_targets(ScopeDimension::SettlementParty, ids, executor).await?
                },
                ScopeTarget::Warehouses(ids) => {
                    self.targets.validate_targets(ScopeDimension::Warehouse, ids, executor).await?
                },
                _ => {},
            }
        }
        Ok(())
    }

    // 范围校验通过后，在原事务确认旧配置可转换，再按动作更新或新增；不触碰未选动作，也不软删除占用唯一键的行。
    async fn replace_person_scope(
        &self,
        user: &str,
        req: &PersonScopeCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut existing =
            self.db.person_data_scopes().for_person(user, Some(&req.resource), None, executor).await?;
        req.ensure_legacy_conversion(&existing)?;

        for action in &req.actions {
            let expression = req.expression(action);
            if let Some(scope) = existing.iter_mut().find(|s| s.action == *action) {
                scope.expression = expression;
                self.db.person_data_scopes().update(scope, executor).await?;
            } else {
                let scope = PersonDataScope {
                    base: BaseModel::new(id_generator::next_id()),
                    user_id: user.into(),
                    resource: req.resource.clone(),
                    action: action.clone(),
                    expression,
                };
                self.db.person_data_scopes().create(&scope, executor).await?;
            }
        }
        Ok(())
    }
}

// `granted` 只包含同一业务的公共动作，保留无业务资格与部分动作缺失的错误区别。
fn ensure_granted_actions(granted: &[Permission], actions: &[String]) -> Result<()> {
    if granted.is_empty() {
        return Err(Error::ValidationError("此人没有该业务操作权限".into()));
    }
    if actions.iter().any(|action| {
        !granted.iter().any(|permission| {
            permission.action() == action && scope_spec::is_configurable(permission.resource(), action)
        })
    }) {
        return Err(Error::ValidationError("请选择此人当前具备的操作".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_permissions::{customer, purchase_order};

    use super::*;

    #[test]
    fn save_checks_granted_configurable_actions() {
        let list = ["list".into()];
        let list_and_detail = ["list".into(), "detail".into()];
        assert!(ensure_granted_actions(&[customer::LIST, customer::DETAIL], &list_and_detail).is_ok());
        assert!(matches!(
            ensure_granted_actions(&[], &list),
            Err(Error::ValidationError(message)) if message == "此人没有该业务操作权限"
        ));
        for (granted, actions) in
            [(customer::LIST, list_and_detail.as_slice()), (purchase_order::LIST, list.as_slice())]
        {
            assert!(matches!(
                ensure_granted_actions(&[granted], actions),
                Err(Error::ValidationError(message)) if message == "请选择此人当前具备的操作"
            ));
        }
    }
}
