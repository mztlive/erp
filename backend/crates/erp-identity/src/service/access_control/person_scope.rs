//! 原子保存所选操作的附加授权，审计与策略版本同事务提交。
use application_core::AuditActor;
use entity_core::BaseModel;
use persistence_core::Executor;

use super::{AccessControlService, consumers};
use crate::access_control::{DataScope, ScopeDimension, ScopeTargetMode};
use crate::dto::person_scope::SavePersonScopeRequest;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::organization::OrgTree;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, Result};

impl AccessControlService {
    /// 保存人员指定业务操作的完整附加授权列表，不改变角色操作权限。
    /// # 参数
    /// 固定人员、业务操作及完整范围、管理员。
    /// # 返回
    /// 原子保存成功。
    /// # 错误
    /// 越权、过期版本、无效目标或缺动作资格拒绝整次保存。
    pub async fn save_person_scope(
        &self,
        user: &str,
        req: SavePersonScopeRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let action = req.actions.first().ok_or_else(|| Error::ValidationError("请选择操作".into()))?;
        let dimensions = consumers::registration(&req.resource, action)?.required_dimensions;
        let req = req.normalized(dimensions)?;
        let event = self
            .build_audit_event(
                actor,
                "person_data_scope.replace",
                "person_data_scope",
                Some(user.into()),
                vec!["resource".into(), "actions".into(), "expression".into()],
            )
            .await?;
        let service = self.scope_service();
        let (user, actor) = (user.to_owned(), actor.clone());
        self.scope_rbac()?
            .run_authorized_policy_transaction(req.expected_policy_version, move |executor| {
                Box::pin(async move {
                    service.authorize_person_scope(&actor, true, executor).await?;
                    service.validate_person_scope(&user, &req, executor).await?;
                    service.replace_person_scope(&user, &req, executor).await?;
                    service.db.audit_events().create(&event, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    /// 逐操作检查有效资格及完整维度，目标存在性在同一事务内验证。
    async fn validate_person_scope(
        &self,
        user: &str,
        req: &SavePersonScopeRequest,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let options = self.person_business_options(user, executor).await?;
        let option = options
            .iter()
            .find(|o| o.resource == req.resource)
            .ok_or_else(|| Error::ValidationError("此人没有该业务操作权限".into()))?;
        if req.actions.is_empty() || req.actions.iter().any(|a| !option.actions.contains(a)) {
            return Err(Error::ValidationError("请选择此人当前具备的操作".into()));
        }
        for action in &req.actions {
            consumers::registration(&req.resource, action)?;
        }
        for grant in &req.grants {
            for term in &grant.terms {
                let rule = term.rule(&req.resource, &grant.actions[0], user, true)?;
                consumers::validate_binding(&rule.binding)?;
                consumers::validate_scope_type(&req.resource, rule.scope_type)?;
                self.validate_scope_targets(&rule, executor).await?;
            }
        }
        Ok(())
    }

    /// 校验组织及外部对象身份，不能通过ID混用维度。
    async fn validate_scope_targets(&self, scope: &DataScope, executor: &mut dyn Executor) -> Result<()> {
        if scope.binding.target_mode != Some(ScopeTargetMode::Explicit) {
            return Ok(());
        }
        if scope.binding.target_dimension == ScopeDimension::InternalOrg {
            let state = OrganizationRepository::new(&self.db).state(executor).await?;
            let tree = OrgTree::new(&state.units)?;
            for id in &scope.scope_targets {
                if tree.expand(id, false)?.is_empty() {
                    return Err(Error::ValidationError("目标部门已停用".into()));
                }
            }
        } else {
            self.targets
                .as_ref()
                .ok_or_else(|| Error::Forbidden("未装配对象范围校验".into()))?
                .validate_targets(scope.binding.target_dimension, &scope.scope_targets, executor)
                .await?;
        }
        Ok(())
    }

    /// 一次保存替换所选操作，其余业务动作不变；不软删唯一键行。
    async fn replace_person_scope(
        &self,
        user: &str,
        req: &SavePersonScopeRequest,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let existing =
            self.db.person_data_scopes().for_person(user, Some(&req.resource), None, executor).await?;
        req.ensure_legacy_conversion(&existing)?;
        for action in &req.actions {
            let expression = req.expression(action);
            if let Some(old) = existing.iter().find(|s| s.action == *action) {
                let mut scope = old.clone();
                scope.expression = expression;
                self.db.person_data_scopes().update(&mut scope, executor).await?;
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
