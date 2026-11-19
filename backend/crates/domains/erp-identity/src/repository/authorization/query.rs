use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
#[cfg(test)]
use std::{cell::Cell, future::Future};

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::bson::{Bson, Document};
use persistence_core::{ExecutionBoundary, Executor};

use crate::entity::authorization::{AccessContext, AccessRule, MappingFacts, QueryModel, RelationKey};
use crate::ports::AccessMapping;
use crate::{Error, Permission, QualifiedPermissions, Result};

#[cfg(test)]
tokio::task_local! {
    static PROOF_TIME: Cell<Instant>;
}

#[derive(Clone)]
pub(crate) struct AccessProof {
    pub context: AccessContext,
    pub qualified: QualifiedPermissions,
    pub approval_source: Option<ApprovalSource>,
    pub task: bool,
    pub rule: AccessRule,
    pub whole_rule: Option<AccessRule>,
    pub management_filter: Option<ResolvedFilter>,
    pub has_role_scope: bool,
    pub version: String,
    pub valid_until: Option<Instant>,
    pub policy_version: u64,
    pub organization_version: u64,
    pub boundary: ExecutionBoundary,
    pub service: Arc<()>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ResolvedFilter {
    Root,
    Separate { context: Box<AccessContext>, rule: AccessRule },
}

#[derive(Clone)]
pub(crate) struct ApprovalSource {
    pub source: Permission,
    pub required: Vec<Permission>,
}

impl AccessProof {
    // 交付复验先核对凭证所属服务、执行边界及有效期。原快照不重读；非事务读取须重新取证实际依赖。
    pub(crate) fn needs_revalidation(&self, token: &Arc<()>, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_current(executor)?;
        if !Arc::ptr_eq(&self.service, token) {
            return Err(Error::ValidationError("授权结果属于其他服务目录".into()));
        }
        Ok(!self.boundary.is_snapshot())
    }

    // 在授权解析或交付时校验执行器仍属于同一事务和读取阶段。越过原边界返回校验错误，到达有效期截止时刻返回 `DATA_SCOPE_CHANGED`。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        if !self.boundary.is_current(executor) {
            return Err(Error::ValidationError("授权结果不能跨事务或读取阶段使用".into()));
        }

        #[cfg(test)]
        let now = PROOF_TIME.try_with(Cell::get).unwrap_or_else(|_| Instant::now());
        #[cfg(not(test))]
        let now = Instant::now();
        if self.valid_until.is_some_and(|until| now >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权关系已到期".into()));
        }
        Ok(())
    }

    // 在当前异步任务内控制证明校验时钟，退出作用域后恢复墙钟。
    // 返回测试流程的结果。
    //
    // 不返回错误；测试流程的错误由返回值保留。
    //
    // now 为初始时刻，future 为执行真实授权及校验的测试流程。
    #[cfg(test)]
    pub(crate) async fn with_test_time<F: Future>(now: Instant, future: F) -> F::Output {
        PROOF_TIME.scope(Cell::new(now), future).await
    }

    // 将当前测试任务的证明校验时钟推进到指定时刻。
    // 后续证明校验使用指定时刻。
    //
    // 不返回错误；缺少测试时钟作用域时 panic。
    //
    // now 为精确校验时刻；调用必须位于 with_test_time 作用域内。
    #[cfg(test)]
    pub(crate) fn set_test_time(now: Instant) {
        PROOF_TIME.with(|time| time.set(now));
    }
}

/// 编译器生成的查询计划；业务不能自行构造。
#[derive(Clone)]
pub struct CompiledAccess {
    pub(crate) proof: AccessProof,
    pub(crate) model: QueryModel,
    pub(crate) stages: Vec<Document>,
    pub(crate) dependencies: Vec<MappingDependency>,
    pub(crate) version: String,
    pub(crate) sources: BTreeMap<RelationKey, SourcePlan>,
    pub(crate) whole: Option<Vec<Document>>,
    pub(super) management: Option<ManagementPlan>,
}

#[derive(Clone)]
pub(super) enum ManagementPlan {
    Root,
    Separate(Vec<Document>),
}

#[derive(Clone)]
pub(crate) struct MappingDependency {
    pub mapping: Arc<dyn AccessMapping>,
    pub model: QueryModel,
    pub context: AccessContext,
    pub facts: MappingFacts,
    pub key: Bson,
}

impl MappingDependency {
    // 非事务交付时沿原执行器重新准备相同映射，比较模型、动态事实、主键表达式、修订号和有效期，不用旧数据假装当前仍有效。
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

#[derive(Clone)]
pub(crate) struct SourcePlan {
    pub model: QueryModel,
    pub stages: Vec<Document>,
}

impl CompiledAccess {
    /// 取得本次工作项查询的管理范围过滤，用于给已授权展示批次计算按钮资格。非工作项凭证拒绝；命令仍须单独执行 `require`。
    pub fn management_stages(&self) -> Result<&[Document]> {
        match &self.management {
            Some(ManagementPlan::Root) => Ok(self.stages()),
            Some(ManagementPlan::Separate(stages)) => Ok(stages),
            None => Err(Error::ValidationError("此查询没有管理动作过滤".into())),
        }
    }

    /// 供固定根投影核对来源份额的集合和身份描述。没有登记该来源份额时拒绝；描述本身不签发来源对象凭证。
    pub fn source_model(&self, relation: &RelationKey) -> Result<&QueryModel> {
        self.sources
            .get(relation)
            .map(|source| &source.model)
            .ok_or_else(|| Error::ValidationError("本查询不提供此来源份额".into()))
    }

    /// 供固定根投影读取来源份额时执行同次编译的数据库过滤。入口未登记该来源输出时拒绝，来源阶段不能扩大根输出边界。
    pub fn source_stages(&self, relation: &RelationKey) -> Result<&[Document]> {
        self.sources
            .get(relation)
            .map(|source| source.stages.as_slice())
            .ok_or_else(|| Error::ValidationError("本查询不提供此来源份额".into()))
    }

    /// 复合资金入口需要展示整账时，取得同次取证生成的根过滤；完整金额资格与全部真实来源均须通过。普通入口或份额子凭证不能用于整账读取。
    pub fn whole_stages(&self) -> Result<&[Document]> {
        self.whole.as_deref().ok_or_else(|| Error::ValidationError("此查询没有完整根读取计划".into()))
    }

    // 身份域展示是否存在正向角色范围。
    // 本次范围取证存在正向角色条款时为 true。
    //
    // 个人限制和查询结果为空不能替代本标记。
    pub(crate) fn has_role_scope(&self) -> bool {
        self.proof.has_role_scope
    }

    /// 借用凭证绑定的真实账号，供同一授权批次补充展示资格。账号有效性仍须在交付时复验。
    pub fn actor(&self) -> &AuditActor {
        &self.proof.context.actor
    }

    /// 读取凭证绑定的固定入口权限，供仓储核对用途；不能据此把凭证转给另一操作。
    pub fn permission(&self) -> &Permission {
        &self.proof.context.permission
    }

    /// 返回本次身份资格取证使用的权限策略修订。
    /// 与查询凭证同次读取的策略修订，不能代替 validate。
    ///
    /// 不执行 I/O。
    pub fn policy_version(&self) -> u64 {
        self.proof.policy_version
    }

    /// 返回本次范围解析使用的组织修订。
    /// 依赖组织的范围修订；未读取组织范围时为 0。
    ///
    /// 不执行 I/O。
    pub fn organization_version(&self) -> u64 {
        self.proof.organization_version
    }

    /// 读取编译时冻结的授权时点；范围和业务映射使用这一时点解释有效关系，不能改用当前墙钟重算条件。
    pub fn as_of(&self) -> Instant {
        self.proof.context.as_of
    }

    /// 仓储执行授权查询前核对凭证仍属于原执行器的同一事务和读取阶段，且未到任何有效期截止点。
    /// 此步不访问数据库，不能代替结果交付前的 `AuthorizationService::validate`。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        self.proof.ensure_current(executor)?;
        for dependency in &self.dependencies {
            dependency.facts.validate(false, Instant::now())?;
        }
        Ok(())
    }

    /// 读取身份、范围及实际业务映射依赖的稳定摘要，供跨页游标检查授权上下文是否变化。摘要不是通用凭证，仓储执行前仍须检查边界，交付前仍须复验。
    pub fn version(&self) -> String {
        self.version.clone()
    }

    /// 借用固定模型的完整 MongoDB 授权阶段，包括最终权限过滤及临时字段清理。仓储先执行这些阶段，再做分页、计数或汇总；查询交付前仍须统一复验。
    pub fn stages(&self) -> &[Document] {
        &self.stages
    }

    /// 借用凭证绑定的根集合、行身份及模型描述，供仓储检查查询接入是否匹配；不能换到其他资源执行。
    pub fn model(&self) -> &QueryModel {
        &self.model
    }
}

impl fmt::Debug for CompiledAccess {
    // 打印授权凭证的诊断信息时只暴露查询模型，不打印账号身份或敏感业务事实。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("CompiledAccess").field("model", &self.model).finish_non_exhaustive()
    }
}
