use std::sync::Arc;
use std::{fmt, ptr};

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::bson::{Bson, Document};
use persistence_core::Executor;

use crate::entity::authorization::{AccessContext, AccessRule, AccessSubject, MappingFacts, QueryModel};
use crate::ports::AccessMapping;
use crate::repository::authorization::compile;
use crate::{Error, Result};

/// 只有授权服务能生成的完整查询约束。
pub struct AuthorizedQuery {
    pub(crate) proof: AccessProof,
    pub(crate) mapping: Arc<dyn AccessMapping>,
    pub(crate) model: QueryModel,
}

#[derive(Clone)]
pub(crate) struct AccessProof {
    pub context: AccessContext,
    pub subject: AccessSubject,
    pub rule: AccessRule,
    pub version: String,
    pub valid_until: Option<Instant>,
    pub session: Option<Document>,
    pub registry: Arc<()>,
}

impl AccessProof {
    /// 执行器和有效期必须与本次解析阶段一致。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        if self.session != executor.session().map(|session| session.id().clone()) {
            return Err(Error::ValidationError("授权上下文不能更换执行器会话".into()));
        }
        if self.valid_until.is_some_and(|until| Instant::now() >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权关系已到期".into()));
        }
        Ok(())
    }
}

impl AuthorizedQuery {
    /// 用服务端登记的映射编译查询约束。
    /// # 参数
    /// mapping 必须是本次解析绑定的映射；executor 为原执行器。
    /// # 返回
    /// 完整查询阶段及复验所需事实。
    /// # 错误
    /// 映射错配、事实缺失、有效期或会话变化拒绝。
    pub async fn compile(
        &self,
        mapping: &dyn AccessMapping,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        self.proof.ensure_current(executor)?;
        if !ptr::eq(mapping, self.mapping.as_ref()) || *mapping.model() != self.model {
            return Err(Error::ValidationError("授权查询与 Mapping 不匹配".into()));
        }
        let compiled = compile(self, executor).await?;
        compiled.ensure_current(executor)?;
        Ok(compiled)
    }

    /// 读取授权版本，用于跨页版本比较。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本次身份、范围及读取授权版本。
    /// # 错误
    /// 不返回错误；不证明业务映射事实仍有效。
    pub fn version(&self) -> &str {
        &self.proof.version
    }
}

/// 编译器生成的查询计划；业务不能自行构造。
pub struct CompiledAccess {
    pub(crate) proof: AccessProof,
    pub(crate) model: QueryModel,
    pub(crate) stages: Vec<Document>,
    pub(crate) dependencies: Vec<MappingDependency>,
}

pub(crate) struct MappingDependency {
    pub mapping: Arc<dyn AccessMapping>,
    pub model: QueryModel,
    pub context: AccessContext,
    pub facts: MappingFacts,
    pub key: Bson,
}

impl MappingDependency {
    /// 重新准备同一映射，比较全部事实、版本和到期边界。
    pub(crate) async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        if *self.mapping.model() != self.model {
            return Err(Error::ValidationError("授权 Mapping 模型已改变".into()));
        }
        self.facts.validate(executor.session().is_some(), Instant::now())?;
        let current = self.mapping.prepare(&self.context, executor).await?;
        current.validate(executor.session().is_some(), Instant::now())?;
        if current != self.facts || self.mapping.key(&self.context, &current)? != self.key {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：业务授权事实已变化".into()));
        }
        Ok(())
    }
}

impl CompiledAccess {
    /// 返回查询凭证绑定的真实账号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 只读身份，供当前展示批次补充功能资格。
    /// # 错误
    /// 无；账号有效性仍由 validate 重验。
    pub fn actor(&self) -> &AuditActor {
        &self.proof.context.actor
    }

    /// 在所有异步复验完成后检查整体有效期。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        self.proof.ensure_current(executor)?;
        for dependency in &self.dependencies {
            dependency.facts.validate(executor.session().is_some(), Instant::now())?;
        }
        Ok(())
    }

    /// 读取包含业务映射依赖的完整版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 用于分页游标复验的身份与映射事实摘要。
    /// # 错误
    /// 不返回错误；不能替代执行前后的 validate。
    pub fn version(&self) -> String {
        let mut parts = vec![self.proof.version.clone()];
        for dependency in &self.dependencies {
            parts.push(format!("{:?}:{:?}:{:?}", dependency.model, dependency.facts, dependency.key));
        }
        format!("{:x}", md5::compute(format!("{parts:?}")))
    }

    /// 读取完整授权阶段，必须在分页、计数和汇总前执行。
    /// # 参数
    /// 无。
    /// # 返回
    /// 只读 MongoDB 聚合阶段；包含最终权限过滤与临时字段清理。
    /// # 错误
    /// 不返回错误；返回业务结果前仍须调用统一复验。
    pub fn stages(&self) -> &[Document] {
        &self.stages
    }

    /// 返回绑定的查询模型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 根集合及行身份，不能用于其他资源仓储。
    /// # 错误
    /// 不返回错误。
    pub fn model(&self) -> &QueryModel {
        &self.model
    }
}

impl fmt::Debug for CompiledAccess {
    /// 诊断只输出查询模型，不输出身份及业务事实。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("CompiledAccess").field("model", &self.model).finish_non_exhaustive()
    }
}
