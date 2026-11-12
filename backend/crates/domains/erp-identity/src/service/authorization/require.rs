use std::collections::BTreeSet;

use application_core::AuditActor;
use persistence_core::Executor;

use super::targets::{Target, TargetFacts};
use super::{AuthorizationService, rules};
use crate::entity::access_control::scope_spec;
use crate::entity::authorization::{AccessContext, AccessRule, AccessTarget, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, Result};

impl AuthorizationService {
    /// 检查固定操作及完整目标；不返回可供后续写入复用的凭证。
    /// # 参数
    /// actor、permission 为真实身份及操作；target 为服务端目标；executor 为原执行器。
    /// # 返回
    /// 全部目标、归属及精确任务要求通过时返回成功。
    /// # 错误
    /// 未知操作、不支持目标、缺权、事实缺失、越权或依赖变化均拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        executor.next_read();
        let kind = scope_spec::kind(permission)?;
        let form = target.form();
        rules::ensure_target(permission, kind, form)?;
        if !rules::is_read(permission) && !executor.boundary().is_some_and(|boundary| boundary.is_snapshot())
        {
            return Err(Error::ValidationError("写入授权必须使用快照事务".into()));
        }
        if matches!(target, AccessTarget::Operation) {
            let boundary = executor.boundary();
            self.qualify(actor, permission, executor).await?;
            if boundary.is_some_and(|boundary| !boundary.is_current(executor)) {
                return Err(Error::ValidationError("操作检查跨越执行阶段".into()));
            }
            return Ok(());
        }
        let proof = self.resolve(actor, permission, None, None, executor).await?;
        let facts =
            TargetFacts::load(&self.facts, &proof.context, &proof.rule, target.borrowed()?, executor).await?;
        let grants = self.target_grants(&proof.context, &proof.rule, &facts, executor).await?;
        facts.require(&proof.context, &proof.rule, &grants, executor)?;
        self.validate_proof(&proof, executor).await?;
        facts.revalidate(&self.facts, &proof.context, executor).await?;
        facts.require(&proof.context, &proof.rule, &grants, executor)?;
        proof.ensure_current(executor)
    }

    /// 授权行只读取本次目标及真实来源；不读取账号历史授权全集。
    async fn target_grants(
        &self,
        context: &AccessContext,
        rule: &AccessRule,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<(ResourceKind, String)>> {
        let flattened = facts.flattened()?;
        let mut resources = BTreeSet::new();
        rule.grant_resources(&mut resources);
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

impl AccessTarget {
    fn form(&self) -> rules::TargetForm {
        match self {
            Self::Operation => rules::TargetForm::Operation,
            Self::Records(_) => rules::TargetForm::Record,
            Self::Candidate(_) => rules::TargetForm::Candidate,
            Self::OwnershipChange { .. } => rules::TargetForm::OwnershipChange,
            Self::Task { .. } => rules::TargetForm::Task,
        }
    }

    fn borrowed(&self) -> Result<Target<'_>> {
        Ok(match self {
            Self::Records(ids) => Target::Records(ids),
            Self::Candidate(candidate) => Target::Candidate(candidate),
            Self::OwnershipChange { id, candidate } => Target::OwnershipChange { id, candidate },
            Self::Task { id, version } => Target::Task { id, version },
            Self::Operation => return Err(Error::ValidationError("无对象目标不能读取事实".into())),
        })
    }
}
