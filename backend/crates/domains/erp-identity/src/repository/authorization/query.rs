use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
#[cfg(test)]
use std::{cell::Cell, future::Future};

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::work_item;
use mongodb::bson::{Bson, Document};
use persistence_core::{ExecutionBoundary, Executor};

use crate::entity::authorization::{AccessContext, AccessRule, MappingFacts, QueryModel, RelationKey};
use crate::ports::AccessMapping;
use crate::service::authorization::rules::sources;
use crate::service::authorization::work_items;
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
    pub management_rule: Option<AccessRule>,
    pub has_role_scope: bool,
    pub version: String,
    pub valid_until: Option<Instant>,
    pub policy_version: u64,
    pub organization_version: u64,
    pub boundary: ExecutionBoundary,
    pub service: Arc<()>,
}

#[derive(Clone)]
pub(crate) struct ApprovalSource {
    pub source: Permission,
    pub required: Vec<Permission>,
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
    /// # 参数
    /// executor 为原授权解析使用的执行器。
    /// # 返回
    /// 原读取边界仍有效且当前时刻早于截止时刻时成功。
    /// # 错误
    /// 跨读取边界返回校验错误；到达截止时刻返回范围变化错误。
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

    /// 在当前异步任务内控制证明校验时钟，退出作用域后恢复墙钟。
    /// # 参数
    /// now 为初始时刻，future 为执行真实授权及校验的测试流程。
    /// # 返回
    /// 返回测试流程的结果。
    /// # 错误
    /// 不返回错误；测试流程的错误由返回值保留。
    #[cfg(test)]
    pub(crate) async fn with_test_time<F: Future>(now: Instant, future: F) -> F::Output {
        PROOF_TIME.scope(Cell::new(now), future).await
    }

    /// 将当前测试任务的证明校验时钟推进到指定时刻。
    /// # 参数
    /// now 为精确校验时刻；调用必须位于 with_test_time 作用域内。
    /// # 返回
    /// 后续证明校验使用指定时刻。
    /// # 错误
    /// 不返回错误；缺少测试时钟作用域时 panic。
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
    pub(crate) management: Option<Vec<Document>>,
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

#[derive(Clone)]
pub(crate) struct SourcePlan {
    pub model: QueryModel,
    pub stages: Vec<Document>,
}

impl CompiledAccess {
    /// 返回本次工作项展示批次的固定完整执行资格。
    /// # 参数
    /// kind、source 为已授权任务的真实种类和来源类型；executor 为原读取执行器。
    /// # 返回
    /// 同次查询的启用角色完整覆盖该种类的固定资格时为 true；不证明任务可办理。
    /// # 错误
    /// 非工作项展示查询、未知种类、跨阶段或到期时拒绝；交付前仍须 validate 原查询。
    pub fn execution_qualified(&self, kind: &str, source: &str, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_work_item_view(executor)?;
        work_items::execution_qualified(kind, source, &self.proof.qualified)
    }

    /// 返回本次工作项展示批次的固定审批决定资格。
    /// # 参数
    /// executor 为生成当前查询的原读取执行器。
    /// # 返回
    /// 同次查询的启用角色同时覆盖抄送历史与提交决定时为 true；不签发办理凭证。
    /// # 错误
    /// 非工作项展示查询、跨阶段或到期时拒绝；交付前仍须 validate 原查询。
    pub fn decision_qualified(&self, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_work_item_view(executor)?;
        Ok(work_items::decision_qualified(&self.proof.qualified))
    }

    /// 展示资格和原查询的身份依赖、最早有效期及读取阶段不可分离。
    fn ensure_work_item_view(&self, executor: &mut dyn Executor) -> Result<()> {
        if !work_items::view_permission(&self.proof.context.permission) {
            return Err(Error::ValidationError("此查询不提供工作项操作展示资格".into()));
        }
        self.ensure_current(executor)
    }

    /// 返回工作项当前读取阶段内固定的管理动作过滤。
    /// # 参数
    /// 无。
    /// # 返回
    /// 与根读取共用资格、范围依赖及有效期的过滤；仅用于当前批次按钮展示。
    /// # 错误
    /// 非工作项入口拒绝；办理命令仍必须单独 require。
    pub fn management_stages(&self) -> Result<&[Document]> {
        if self.proof.context.permission == work_item::MANAGE {
            return Ok(&self.stages);
        }
        self.management.as_deref().ok_or_else(|| Error::ValidationError("此查询没有管理动作过滤".into()))
    }

    /// 取得固定来源模型的只读描述，不签发来源对象凭证。
    /// # 参数
    /// relation 为启动时绑定的来源关系。
    /// # 返回
    /// 集合、主键及静态能力描述；来源阶段仅供原根用例的固定投影使用。
    /// # 错误
    /// 不提供该来源份额时返回接入错误。
    pub fn source_model(&self, relation: &RelationKey) -> Result<&QueryModel> {
        self.sources
            .get(relation)
            .map(|source| &source.model)
            .ok_or_else(|| Error::ValidationError("本查询不提供此来源份额".into()))
    }

    /// 取得固定来源的数据库过滤阶段。
    /// # 参数
    /// relation 为启动时绑定的完整来源关系。
    /// # 返回
    /// 同次取证生成的来源阶段，不改变根输出边界。
    /// # 错误
    /// 本入口不能输出该来源时拒绝。
    pub fn source_stages(&self, relation: &RelationKey) -> Result<&[Document]> {
        self.sources
            .get(relation)
            .map(|source| source.stages.as_slice())
            .ok_or_else(|| Error::ValidationError("本查询不提供此来源份额".into()))
    }

    /// 取得同次资格及范围生成的完整根过滤。
    /// # 参数
    /// 无。
    /// # 返回
    /// 整账资格与全部真实来源均满足时才命中的固定阶段。
    /// # 错误
    /// 非复合资金入口或来源子凭证不能用于整账读取。
    pub fn whole_stages(&self) -> Result<&[Document]> {
        self.whole.as_deref().ok_or_else(|| Error::ValidationError("此查询没有完整根读取计划".into()))
    }

    /// 身份域展示是否存在正向角色范围。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本次范围取证存在正向角色条款时为 true。
    /// # 错误
    /// 无；个人限制和查询结果为空不能替代本标记。
    pub(crate) fn has_role_scope(&self) -> bool {
        self.proof.has_role_scope
    }

    /// 返回同次取证的完整金额功能资格，不授予来源对象访问。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前固定复合入口的完整功能资格成立时为 true，仍须执行完整来源过滤。
    /// # 错误
    /// 无；此只读信息不能代替查询边界、来源条件及最终交付检查。
    pub fn has_whole_qualification(&self) -> bool {
        self.whole.is_some()
            && sources::whole_qualified(&self.proof.context.permission, &self.proof.qualified)
    }

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
