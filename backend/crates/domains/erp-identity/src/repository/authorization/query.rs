use std::fmt;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::bson::{Bson, Document};
use persistence_core::{ExecutionBoundary, Executor};

use crate::entity::authorization::{AccessContext, AccessRule, MappingFacts, QueryModel};
use crate::ports::AccessMapping;
use crate::{Error, Permission, Result};

#[derive(Clone)]
pub(crate) struct AccessProof {
    pub context: AccessContext,
    pub rule: AccessRule,
    pub version: String,
    pub valid_until: Option<Instant>,
    pub policy_version: u64,
    pub organization_version: u64,
    pub boundary: ExecutionBoundary,
    pub service: Arc<()>,
}

impl AccessProof {
    /// 事务快照仅检查当前边界和服务；非事务必须重读实际依赖。
    pub(crate) fn needs_revalidation(&self, token: &Arc<()>, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_current(executor)?;
        if !Arc::ptr_eq(&self.service, token) {
            return Err(Error::ValidationError("授权结果属于其他服务目录".into()));
        }
        Ok(!self.boundary.is_snapshot())
    }

    /// 执行器和有效期必须与本次解析阶段一致。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        if !self.boundary.is_current(executor) {
            return Err(Error::ValidationError("授权结果不能跨事务或读取阶段使用".into()));
        }
        if self.valid_until.is_some_and(|until| Instant::now() >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权关系已到期".into()));
        }
        Ok(())
    }
}

/// 编译器生成的查询计划；业务不能自行构造。
pub struct CompiledAccess {
    pub(crate) proof: AccessProof,
    pub(crate) model: QueryModel,
    pub(crate) stages: Vec<Document>,
    pub(crate) dependencies: Vec<MappingDependency>,
    pub(crate) version: String,
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
        self.facts.validate(false, Instant::now())?;
        let preparation = self
            .mapping
            .preparation()
            .ok_or_else(|| Error::ValidationError("动态 Mapping 的准备能力已改变".into()))?;
        let current = preparation.prepare(&self.context, executor).await?;
        current.validate(false, Instant::now())?;
        if current != self.facts || self.mapping.key(&self.context, Some(&current))? != self.key {
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

    /// 返回读取凭证绑定的入口权限。
    /// # 参数
    /// 无。
    /// # 返回
    /// 服务端登记并已核验的具体权限。
    /// # 错误
    /// 无；使用凭证前后仍须复验。
    pub fn permission(&self) -> &Permission {
        &self.proof.context.permission
    }

    /// 返回本次身份资格取证使用的权限策略修订。
    /// # 参数
    /// 无。
    /// # 返回
    /// 与查询凭证同次读取的策略修订，不能代替 validate。
    /// # 错误
    /// 不执行 I/O。
    pub fn policy_version(&self) -> u64 {
        self.proof.policy_version
    }

    /// 返回本次范围解析使用的组织修订。
    /// # 参数
    /// 无。
    /// # 返回
    /// 依赖组织的范围修订；未读取组织范围时为 0。
    /// # 错误
    /// 不执行 I/O。
    pub fn organization_version(&self) -> u64 {
        self.proof.organization_version
    }

    /// 返回统一授权冻结的解释时点。
    /// # 参数
    /// 无。
    /// # 返回
    /// 与范围及业务映射使用的同一授权时点。
    /// # 错误
    /// 不执行 I/O。
    pub fn as_of(&self) -> Instant {
        self.proof.context.as_of
    }

    /// 在仓储执行前核对原事务、读取阶段和全部有效期，不读取数据库。
    /// # 参数
    /// executor 为本次读取使用的原执行器。
    /// # 返回
    /// 当前边界及有效期内返回成功。
    /// # 错误
    /// 跨事务、跨写入阶段或到期时拒绝。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        self.proof.ensure_current(executor)?;
        for dependency in &self.dependencies {
            dependency.facts.validate(false, Instant::now())?;
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
        self.version.clone()
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
