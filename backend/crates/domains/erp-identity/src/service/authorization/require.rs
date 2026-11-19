use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_permissions::{approval_instance, internal};
use persistence_core::Executor;

use super::targets::{SourceEvidence, TargetFacts};
use super::{AuthorizationService, approvals, definitions, operations, rules};
use crate::entity::authorization::{AccessContext, AccessRule, AccessTarget, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, Result};

impl AuthorizationService {
    /// 在业务读取或写入前检查当前身份是否有权对服务端确认的目标执行固定操作。写入、精确任务、带来源的入口及无对象操作须沿用快照事务。
    ///
    /// 目标形状不受支持、资格不足、必需事实缺失或授权依赖变化均拒绝；成功只证明本次检查，不能作为后续写入凭证。
    pub async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        executor.next_read();
        rules::ensure_target(permission, &target)?;
        let operation = matches!(&target, AccessTarget::Operation);
        if (operation
            || matches!(&target, AccessTarget::Task { .. })
            || !rules::is_read(permission)
            || operations::source(permission).is_some())
            && !executor.boundary().is_some_and(|boundary| boundary.is_snapshot())
        {
            let message = if operation && rules::is_read(permission) {
                "无对象操作及其业务读取必须使用快照事务"
            } else {
                "写入授权必须使用快照事务"
            };
            return Err(Error::ValidationError(message.into()));
        }

        if permission == &internal::WORK_ITEM_EXECUTION_QUALIFICATION {
            return self.require_work_item_qualification(actor, &target, executor).await;
        }

        if matches!(&target, AccessTarget::Candidate(_)) && definitions::entry(permission).is_some() {
            return self.require_definition(actor, permission, &target, executor).await;
        }

        if operation {
            let boundary = executor.boundary();
            let qualified = self.qualify(actor, permission, executor).await?;
            self.require_operation_scope(actor, permission, &qualified, executor).await?;
            if boundary.is_some_and(|boundary| !boundary.is_current(executor)) {
                return Err(Error::ValidationError("操作检查跨越执行阶段".into()));
            }
            return Ok(());
        }

        self.require_target(actor, permission, &target, executor).await
    }

    // 验证已有记录、候选归属或精确任务时，加载完整根对象、实际来源及本次用到的记录授权。
    // 异步取证结束后复核依赖、原执行阶段与有效期，不能凭其他放行分支跳过必需事实。
    async fn require_target(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        target: &AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let proof = self
            .resolve_target(
                actor,
                permission,
                None,
                None,
                matches!(target, AccessTarget::Task { .. }),
                executor,
            )
            .await?;

        let facts =
            TargetFacts::load(&self.facts, &proof.context, &proof.resolved.rules.root, target, executor)
                .await?;

        let sources = if approvals::supports(permission) {
            SourceEvidence::from_proofs(self.approval_sources(&proof, &facts, executor).await?)
        } else if permission == &approval_instance::SUBMIT_DECISION {
            SourceEvidence::from_proofs(self.approval_decision_sources(&proof, &facts, executor).await?)
        } else {
            self.work_item_sources(&proof, &facts, executor).await?
        };

        let grants = self.target_grants(&proof.context, &proof.resolved.rules.root, &facts, executor).await?;
        facts.require(&proof.context, &proof.resolved.rules.root, &grants, executor)?;

        self.validate_proof(&proof, executor).await?;
        facts.revalidate(&self.facts, &proof.context, executor).await?;
        for (context, source_facts) in &sources.facts {
            source_facts.revalidate(&self.facts, context, executor).await?;
        }
        for source in &sources.proofs {
            self.validate_proof(source, executor).await?;
        }

        facts.ensure_current_stage(executor)?;
        for (_, source_facts) in &sources.facts {
            source_facts.ensure_current()?;
        }
        for source in &sources.proofs {
            source.validity.ensure_current(executor)?;
        }
        proof.validity.ensure_current(executor)
    }

    // 只在规则含记录读取补充时，按本次根目标及真实来源的资源和 ID 查询有效授权，不拉取账号的历史授权全集。
    pub(super) async fn target_grants(
        &self,
        context: &AccessContext,
        rule: &AccessRule,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<(ResourceKind, String)>> {
        let mut resources = BTreeSet::new();
        rule.grant_resources(&mut resources);
        if resources.is_empty() {
            facts.ensure_current()?;
            return Ok(BTreeSet::new());
        }
        let flattened = facts.flattened()?;

        let mut grants = BTreeSet::new();
        for resource in resources {
            let ids = flattened
                .iter()
                .filter(|object| object.resource == resource.code())
                .map(|object| object.id.as_str())
                .collect::<Vec<_>>();
            if ids.is_empty() {
                continue;
            }
            let targets = ResourceTargets::new(&ids)?;
            let authorized = self.facilities.grants(context, resource, &targets, executor).await?;
            grants.extend(authorized.into_iter().map(|id| (resource, id)));
        }
        Ok(grants)
    }
}
