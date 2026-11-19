//! 原子应用及已提交命令回放。
use application_core::{AuditActor, CommandReceipt};
use entity_core::BaseModel;
use erp_permissions::access_control;
use persistence_core::Executor;

use super::PolicyBundleService;
use crate::dto::authorization_bundle::{ApplyPolicyRequest, PolicyApplyResult};
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::authorization_bundle::plan::PolicyPlan;
use crate::entity::authorization_bundle::receipt::PolicyReceipt;
use crate::repository::policy_receipt::receipts;
use crate::{AccessControlExt, Error, Result};

impl PolicyBundleService {
    /// 审核通过后，按预览的策略版本和摘要在同一事务中应用授权文件。
    ///
    /// 重放也须通过当前授权；同一操作人、操作号和载荷仅返回原提交结果，异载荷返回冲突。
    /// 新命令在目标失效或权限、审核事实变化时拒绝；任一写入失败时整批回滚。
    pub async fn apply(
        &self,
        mut request: ApplyPolicyRequest,
        actor: AuditActor,
    ) -> Result<PolicyApplyResult> {
        request.document = request.document.normalized()?;
        request.document.validate_catalog(&self.catalog)?;
        request.idempotency_key = request.idempotency_key.trim().to_owned();
        if request.idempotency_key.is_empty() || request.idempotency_key.len() > 128 {
            return Err(Error::ValidationError("操作号必须为1至128字节".into()));
        }

        let command = CommandReceipt::from_payload(
            "authorization-policy-",
            actor.id(),
            "authorization_policy.apply",
            "authorization_policy",
            &request.idempotency_key,
            &request,
        )?;

        let service = self.clone();
        self.access
            .rbac
            .clone()
            .run_optional_policy_transaction(Some(request.expected_policy_version), move |executor| {
                Box::pin(async move { service.apply_steps(request, command, actor, executor).await })
            })
            .await
    }

    // 在应用事务中按实际文件重新授权，再查幂等回执。新命令核对策略版本和审核摘要后写入；有配置变更才推进策略版本。
    async fn apply_steps(
        &self,
        request: ApplyPolicyRequest,
        command: CommandReceipt,
        actor: AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(PolicyApplyResult, bool)> {
        let authorization = self
            .access
            .authorization
            .require_policy_bundle(
                &actor,
                &access_control::POLICY_BUNDLE_APPLY,
                Some(&request.document),
                executor,
            )
            .await?;

        if let Some(receipt) = receipts(&self.access.db).find_by_id(command.id(), executor).await? {
            return Ok((receipt.replay(&command)?, false));
        }

        if authorization.scope.policy_version != request.expected_policy_version {
            return Err(Error::ConflictError("授权版本已变化，请重新预览".into()));
        }
        let plan = self.plan(request.document, &actor, &authorization.scope, executor).await?;
        if plan.preview.review_hash != request.review_hash {
            return Err(Error::ConflictError("审核文件或目标事实已变化，请重新预览".into()));
        }

        let change_count = plan.preview.changes.len();
        let changed = change_count > 0;
        let policy_version = request
            .expected_policy_version
            .checked_add(u64::from(changed))
            .ok_or_else(|| Error::ConflictError("授权版本超限".into()))?;

        self.persist(plan, &actor, command.id(), executor).await?;

        let result = PolicyApplyResult {
            command_id: command.id().into(),
            policy_version,
            change_count,
            replayed: false,
        };
        receipts(&self.access.db).create(&PolicyReceipt::new(&command, result.clone())?, executor).await?;
        Ok((result, changed))
    }

    // 审核摘要复验通过后，沿原授权事务写入角色、绑定、人员范围及审计，不另开事务；空计划跳过这些写入。
    async fn persist(
        &self,
        plan: PolicyPlan,
        actor: &AuditActor,
        command_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if plan.preview.changes.is_empty() {
            return Ok(());
        }

        let rbac = self.access.rbac.clone();
        let event = self
            .access
            .build_audit_event(
                actor,
                "authorization_policy.apply",
                "authorization_policy",
                Some(command_id.into()),
                vec!["roles".into(), "bindings".into(), "data_scopes".into()],
            )
            .await?;

        for role in plan.roles {
            rbac.write_bundle_role(role, actor, executor).await?;
        }
        for binding in plan.bindings {
            rbac.write_bundle_binding(binding, actor, executor).await?;
        }

        for scope in plan.scopes {
            match scope.existing {
                Some(mut row) => {
                    row.expression = scope.expression;
                    self.access.db.person_data_scopes().update(&mut row, executor).await?;
                },
                None => {
                    let row = PersonDataScope {
                        base: BaseModel::new(id_generator::next_id()),
                        user_id: scope.user_id,
                        resource: scope.resource,
                        action: scope.action,
                        expression: scope.expression,
                    };
                    self.access.db.person_data_scopes().create(&row, executor).await?;
                },
            }
        }

        self.access.db.audit_events().create(&event, executor).await?;
        Ok(())
    }
}
