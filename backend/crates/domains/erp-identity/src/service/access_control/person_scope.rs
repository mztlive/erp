//! 原子保存所选操作的附加授权，审计与策略版本同事务提交。

use application_core::AuditActor;
use entity_core::BaseModel;
use persistence_core::Executor;

use super::AccessControlService;
use crate::access_control::ScopeDimension;
use crate::dto::person_scope::PersonScopeCommand;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::access_control::scope_target::ScopeTarget;
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, Result};

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
                    let access = service.authorize_person_scope(&actor, true, executor).await?;
                    service.validate_person_scope(&user, &req, &access.organizations, executor).await?;

                    service.replace_person_scope(&user, &req, executor).await?;
                    service.db.audit_events().create(&event, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    // 写入前重验人员仍具备所选可配置动作，再沿原事务核对命令中的目标；必需维度已在命令规范化时校验。
    async fn validate_person_scope(
        &self,
        user: &str,
        req: &PersonScopeCommand,
        organizations: &OrganizationState,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let options = self.person_business_options_for(user, Some(&req.resource), executor).await?;
        let option = options
            .iter()
            .find(|o| o.resource == req.resource)
            .ok_or_else(|| Error::ValidationError("此人没有该业务操作权限".into()))?;
        if req.actions.iter().any(|a| !option.configurable_actions.contains(a)) {
            return Err(Error::ValidationError("请选择此人当前具备的操作".into()));
        }

        self.validate_person_scope_targets(req, organizations, executor).await
    }

    /// 保存或审核配置前，沿原事务按输入顺序核对规范化命令引用的目标是否存在且适用于指定维度。
    /// 首次遇到明确部门目标才构建组织树；部门失效、维度不符、外域校验未装配或读取失败时拒绝。
    pub(super) async fn validate_person_scope_targets(
        &self,
        req: &PersonScopeCommand,
        organizations: &OrganizationState,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut tree = None;
        for target in req.targets() {
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
