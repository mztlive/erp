use std::collections::BTreeSet;

use application_core::AuditActor;
use persistence_core::Executor;

use super::{AccessPolicy, AuthorizationService};
use crate::entity::authorization::{
    AccessSubject, AccessTarget, ObjectFacts, ProposedFacts, ResourceTargets,
};
use crate::repository::authorization::AccessProof;
use crate::repository::resource_grants;
use crate::{Error, Permission, Result};

impl AuthorizationService {
    /// 检查完整操作目标；任一必要对象失败则整体拒绝。
    /// # 参数
    /// actor、permission 为服务端身份与实际操作；target 为完整目标；executor 为原执行器。
    /// # 返回
    /// 本次授权全部通过时成功，不产生长期凭证。
    /// # 错误
    /// 缺少资格、对象、来源、版本或正确登记时拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match target {
            AccessTarget::Operation => {
                let entry = self.registry.entry(permission, &AccessSubject::Direct)?;
                if !matches!(entry.policy, AccessPolicy::Operation) {
                    return Err(Error::ValidationError("本操作必须提供对象".into()));
                }
                self.identity(actor, &entry, executor).await.map(|_| ())
            },
            AccessTarget::Records { subject, ids } => {
                self.records(actor, permission, subject, ids, executor).await
            },
            AccessTarget::Proposed { subject, facts } => {
                self.proposed(actor, permission, subject, facts, executor).await
            },
        }
    }

    /// 根对象身份和数量必须与完整请求集合一致。
    async fn records(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        subject: AccessSubject,
        ids: ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if ids.ids().is_empty() {
            return Err(Error::ValidationError("授权目标不能为空".into()));
        }
        let entry = self.registry.entry(permission, &subject)?;
        let port =
            entry.facts.clone().ok_or_else(|| Error::ValidationError("操作缺少对象事实端口".into()))?;
        let query = self.resolve(actor, entry, executor).await?;
        let objects = port.load(&query.context, &ids, executor).await?;
        ids.ensure_objects(&query.context.resource, &objects)?;
        self.check_objects(&query, &objects, executor).await?;
        self.validate_proof(&query, executor).await?;
        if port.load(&query.context, &ids, executor).await? != objects {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：对象事实已改变".into()));
        }
        for object in &objects {
            object.flatten(&mut Vec::new(), 0)?;
        }
        query.ensure_current(executor)?;
        Ok(())
    }

    /// 拟创建对象由业务端口核实，不接受直接提交的授权布尔值。
    async fn proposed(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        subject: AccessSubject,
        facts: ProposedFacts,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let entry = self.registry.entry(permission, &subject)?;
        if !entry.allow_proposed {
            return Err(Error::ValidationError("本操作不接受拟创建对象".into()));
        }
        let port =
            entry.facts.clone().ok_or_else(|| Error::ValidationError("操作缺少对象事实端口".into()))?;
        let query = self.resolve(actor, entry, executor).await?;
        let object = port.proposed(&query.context, &facts, executor).await?;
        if object.resource != query.context.resource {
            return Err(Error::ValidationError("拟创建对象资源错配".into()));
        }
        object.flatten(&mut Vec::new(), 0)?;
        if !query.rule.without_reads().allows_object(&query.context, &object, &BTreeSet::new())? {
            return Err(Error::Forbidden("无权创建此对象".into()));
        }
        self.validate_proof(&query, executor).await?;
        if port.proposed(&query.context, &facts, executor).await? != object {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：拟创建对象事实已改变".into()));
        }
        object.flatten(&mut Vec::new(), 0)?;
        query.ensure_current(executor)?;
        Ok(())
    }

    /// 批量读取本次目标的授权，查询规模不随账号历史授权增长。
    async fn check_objects(
        &self,
        query: &AccessProof,
        objects: &[ObjectFacts],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut flattened = Vec::new();
        for object in objects {
            object.flatten(&mut flattened, 0)?;
        }
        let mut resources = BTreeSet::new();
        query.rule.grant_resources(&mut resources);
        let mut grants = BTreeSet::new();
        for resource in resources {
            let ids = flattened
                .iter()
                .filter(|object| object.resource == resource.code())
                .map(|object| object.id.as_str())
                .collect::<Vec<_>>();
            let targets = ResourceTargets::new(&ids)?;
            let authorized =
                resource_grants::target_ids(&self.db, query.context.actor.id(), resource, &targets, executor)
                    .await?;
            grants.extend(authorized.into_iter().map(|id| (resource, id)));
        }
        for object in objects {
            if !query.rule.allows_object(&query.context, object, &grants)? {
                return Err(Error::Forbidden("操作包含不可访问的对象".into()));
            }
        }
        Ok(())
    }
}
