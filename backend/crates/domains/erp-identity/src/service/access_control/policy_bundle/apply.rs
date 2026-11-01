//! 原子应用及已提交命令回放。
use application_core::{AuditActor, CommandReceipt};
use entity_core::BaseModel;
use persistence_core::Executor;

use super::PolicyBundleService;
use crate::dto::authorization_bundle::{ApplyPolicyRequest, PolicyApplyResult};
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::authorization_bundle::plan::PolicyPlan;
use crate::entity::authorization_bundle::receipt::PolicyReceipt;
use crate::repository::policy_receipt::receipts;
use crate::{AccessControlExt, Error, Result};

impl PolicyBundleService {
    /// 根据原审核摘要原子应用文件。
    /// # 参数
    /// request 为文件、版本、摘要及操作号；actor 为认证身份。
    /// # 返回
    /// 本次或原提交结果；重放不重写授权。
    /// # 错误
    /// 版本漂移、同键异载荷、越权、目标失效或任一写入失败时拒绝。
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
            .scope_rbac()?
            .run_optional_policy_transaction(Some(request.expected_policy_version), move |executor| {
                Box::pin(async move { service.apply_steps(request, command, actor, executor).await })
            })
            .await
    }

    /// 完整重验先于写入；回执查询仍在当前权限证明之后。
    async fn apply_steps(
        &self,
        request: ApplyPolicyRequest,
        command: CommandReceipt,
        actor: AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(PolicyApplyResult, bool)> {
        let authorization = self.authorize(&request.document, &actor, executor).await?;
        if let Some(receipt) = receipts(&self.access.db).find_by_id(command.id(), executor).await? {
            return Ok((receipt.replay(&command)?, false));
        }
        if authorization.policy_version != request.expected_policy_version {
            return Err(Error::ConflictError("授权版本已变化，请重新预览".into()));
        }
        let plan = self.plan(request.document, &actor, authorization, executor).await?;
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

    /// 全部写入共享原授权执行器；不调用独立事务入口。
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
        let rbac = self.access.scope_rbac()?;
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
