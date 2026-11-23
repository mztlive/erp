use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
#[cfg(test)]
use std::{cell::Cell, future::Future};

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::bson::Document;
use persistence_core::{ExecutionBoundary, Executor};

use crate::entity::authorization::{AccessContext, AccessRule, QueryModel, RelationKey};
use crate::service::authorization::work_items::DisplayQualifications;
use crate::{Error, Permission, Result};

#[cfg(test)]
tokio::task_local! {
    static PROOF_TIME: Cell<Instant>;
}

/// 服务端取证生成的内部证明，供查询编译及交付校验使用。
#[cfg_attr(test, derive(Clone))]
pub(crate) struct AccessProof {
    /// 冻结账号、入口、资源、模型和授权时点的上下文。
    pub context: AccessContext,
    /// 已解析的范围规则、复验方式及版本。
    pub resolved: ResolvedAccess,
    /// 证明所属服务、执行边界和最早到期时点。
    pub validity: AccessValidity,
    /// 本次角色范围及组织版本的展示元数据。
    pub scope: ScopeMetadata,
}

/// 身份和范围取证后的规则集合，供编译及非事务交付重放使用。
#[derive(Clone)]
pub(crate) struct ResolvedAccess {
    /// 根查询及按需登记的整账、管理规则。
    pub rules: ResolvedRules,
    /// 非事务交付时重做取证使用的固定入口类型。
    pub replay: Replay,
    /// 身份和范围依赖的版本，参与生成查询摘要。
    pub version: String,
}

/// 同一次取证解析的根条件与可选附加过滤。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ResolvedRules {
    /// 限制根查询可见记录的完整授权条件。
    pub root: AccessRule,
    /// 可选的完整金额根规则，可复用根查询规则或独立取证。
    pub whole: Option<WholeRule>,
    /// 可选的工作项管理范围过滤，供已授权结果计算按钮资格。
    pub management: Option<ResolvedFilter>,
}

impl ResolvedRules {
    /// 返回解析后的整账规则，按登记复用根规则或借用独立规则，未登记时返回 `None`。
    pub(crate) fn whole_rule(&self) -> Option<&AccessRule> {
        match self.whole.as_ref()? {
            WholeRule::Root => Some(&self.root),
            WholeRule::Separate(rule) => Some(rule),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum WholeRule {
    Root,
    Separate(
        /// 独立的完整金额读取规则，不沿用根查询的可见性过滤。
        AccessRule,
    ),
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ResolvedFilter {
    Root,
    Separate {
        /// 辅助入口使用的管理动作；其余上下文沿用根查询。
        permission: Permission,
        /// 在辅助入口上下文中已解析的过滤规则。
        rule: AccessRule,
    },
}

#[derive(Clone)]
pub(crate) enum Replay {
    Target,
    ExactTask,
    ApprovalSource {
        /// 审批来源对象用于范围取证的入口权限。
        source: Permission,
        /// 重新取证时仍须满足的附加功能权限。
        required: Vec<Permission>,
    },
}

/// 由本次范围取证产生的角色与组织元数据，不是独立授权凭证。
#[derive(Clone)]
pub(crate) struct ScopeMetadata {
    /// 是否存在正向角色范围条款，不由个人限制或结果条数推断。
    pub has_role_scope: bool,
    /// 参与范围求值的组织修订；未读取组织范围时为 0。
    pub organization_version: u64,
}

/// 授权证明或查询计划有效的服务、读取阶段及时间边界。
#[derive(Clone)]
pub(crate) struct AccessValidity {
    /// 所有相关授权事实最早失效的时点，未设置时无固定期限。
    pub valid_until: Option<Instant>,
    /// 取证所用执行器的原事务及读取阶段标识。
    pub boundary: ExecutionBoundary,
    /// 签发该证明的服务实例标记，按实例身份比较。
    pub service: Arc<()>,
}

impl AccessValidity {
    /// 交付时先校验执行边界、有效期和服务实例；仅非事务路径需要重新读取实际依赖，原快照不重复取证。
    pub(crate) fn needs_revalidation(&self, token: &Arc<()>, executor: &mut dyn Executor) -> Result<bool> {
        self.ensure_current(executor)?;
        if !Arc::ptr_eq(&self.service, token) {
            return Err(Error::ValidationError("授权结果属于其他服务目录".into()));
        }
        Ok(!self.boundary.is_snapshot())
    }

    /// 授权解析、执行或交付时校验原事务及读取阶段；跨边界返回校验错误，到期返回 `DATA_SCOPE_CHANGED`。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        Self::check_stage(
            &self.boundary,
            self.valid_until,
            executor,
            "授权结果不能跨事务或读取阶段使用",
            "DATA_SCOPE_CHANGED：授权关系已到期",
        )
    }

    /// 校验执行阶段边界和有效期；跨边界报 `ValidationError`，到期报 `ConflictError`。
    pub(crate) fn check_stage(
        boundary: &ExecutionBoundary,
        valid_until: Option<Instant>,
        executor: &mut dyn Executor,
        wrong_stage: &str,
        expired: &str,
    ) -> Result<()> {
        if !boundary.is_current(executor) {
            return Err(Error::ValidationError(wrong_stage.into()));
        }
        #[cfg(test)]
        let now = PROOF_TIME.try_with(Cell::get).unwrap_or_else(|_| Instant::now());
        #[cfg(not(test))]
        let now = Instant::now();
        if valid_until.is_some_and(|until| now >= until) {
            return Err(Error::ConflictError(expired.into()));
        }
        Ok(())
    }

    /// 在当前测试任务内以 `now` 代替墙钟执行授权流程，返回流程自身结果，退出作用域后恢复墙钟。
    #[cfg(test)]
    pub(crate) async fn with_test_time<F: Future>(now: Instant, future: F) -> F::Output {
        PROOF_TIME.scope(Cell::new(now), future).await
    }

    /// 修改当前任务后续校验证明使用的测试时钟；必须在 `with_test_time` 内调用，否则会 panic。
    #[cfg(test)]
    pub(crate) fn set_test_time(now: Instant) {
        PROOF_TIME.with(|time| time.set(now));
    }
}

/// 交付时仍需读取的资格，不持有账号或角色实体。
#[derive(Clone, Default)]
pub(crate) struct QueryQualifications {
    /// 同次功能资格取证的策略修订。
    pub(crate) policy_version: u64,
    /// 仅工作项展示入口保存预先判定的按钮资格。
    pub(crate) work_item_display: Option<DisplayQualifications>,
    /// 是否同时具备整账计划及同角色完整金额读取资格。
    pub(crate) whole_qualified: bool,
}

/// 授权编译器签发的 MongoDB 查询计划，业务不能自行构造或改变其绑定。
#[derive(Clone)]
pub struct CompiledAccess {
    /// 签发时的账号、入口、资源、模型和授权时点。
    pub(crate) context: AccessContext,
    /// 同次取证中供查询交付使用的资格摘要。
    pub(crate) qualifications: QueryQualifications,
    /// 查询可使用的服务实例、执行边界及到期时点。
    pub(crate) validity: AccessValidity,
    /// 非事务交付的复验依据，快照路径不保存。
    pub(crate) revalidation: Option<Box<ResolvedAccess>>,
    /// 本次组织修订及正向角色范围标记。
    pub(super) scope: ScopeMetadata,
    /// 凭证固定绑定的根集合、记录身份字段和输出模型。
    pub(crate) model: QueryModel,
    /// 完整 MongoDB 授权管道，含最终过滤及必要的投影和清理。
    pub(crate) stages: Vec<Document>,
    /// 身份、范围、查询模型与过滤阶段的稳定摘要，供跨页检查。
    pub(crate) version: String,
    /// 按关系键索引的可见来源份额计划。
    pub(crate) sources: BTreeMap<RelationKey, SourcePlan>,
    /// 可选的整账根过滤计划，可能复用根管道。
    pub(super) whole: Option<FilterPlan>,
    /// 可选的工作项管理动作过滤计划，可能复用根管道。
    pub(super) management: Option<FilterPlan>,
}

#[derive(Clone)]
pub(super) enum FilterPlan {
    Root,
    Separate(
        /// 为辅助读取单独编译的 MongoDB 授权管道。
        Vec<Document>,
    ),
}

/// 随根查询同次编译的来源份额子计划，不是独立对象凭证。
#[derive(Clone)]
pub(crate) struct SourcePlan {
    /// 该份额引用的来源集合和记录身份描述。
    pub model: QueryModel,
    /// 过滤当前入口可见来源的完整授权管道。
    pub stages: Vec<Document>,
}

impl CompiledAccess {
    /// 获取本次工作项的管理过滤，为已授权展示批次计算按钮资格；未登记时拒绝，办理仍须单独执行 `require`。
    pub fn management_stages(&self) -> Result<&[Document]> {
        match &self.management {
            Some(FilterPlan::Root) => Ok(self.stages()),
            Some(FilterPlan::Separate(stages)) => Ok(stages),
            None => Err(Error::ValidationError("此查询没有管理动作过滤".into())),
        }
    }

    /// 返回指定来源份额的模型供固定根投影核对集合和身份；未登记时拒绝，模型本身不是来源对象凭证。
    pub fn source_model(&self, relation: &RelationKey) -> Result<&QueryModel> {
        self.sources
            .get(relation)
            .map(|source| &source.model)
            .ok_or_else(|| Error::ValidationError("本查询不提供此来源份额".into()))
    }

    /// 返回同次取证编译的来源份额过滤，供固定根投影执行；未登记时拒绝，不得用于扩大根输出边界。
    pub fn source_stages(&self, relation: &RelationKey) -> Result<&[Document]> {
        self.sources
            .get(relation)
            .map(|source| source.stages.as_slice())
            .ok_or_else(|| Error::ValidationError("本查询不提供此来源份额".into()))
    }

    /// 为复合资金入口取得本次整账根过滤；须同时满足完整金额资格及全部真实来源条件。普通入口或份额凭证未登记此计划时拒绝。
    pub fn whole_stages(&self) -> Result<&[Document]> {
        match &self.whole {
            Some(FilterPlan::Root) => Ok(self.stages()),
            Some(FilterPlan::Separate(stages)) => Ok(stages),
            None => Err(Error::ValidationError("此查询没有完整根读取计划".into())),
        }
    }

    /// 读取本次取证是否存在正向角色范围条款，用于身份域展示；不能从个人限制或查询是否为空推断。
    pub(crate) fn has_role_scope(&self) -> bool {
        self.scope.has_role_scope
    }

    /// 借用凭证绑定的真实账号以补充同批次展示资格；交付仍须复验账号有效性。
    pub fn actor(&self) -> &AuditActor {
        &self.context.actor
    }

    /// 取得凭证绑定的固定入口权限供仓储核对，只能用于原操作。
    pub fn permission(&self) -> &Permission {
        &self.context.permission
    }

    /// 返回同次功能资格取证的权限策略修订，不访问数据库，也不替代交付复验。
    pub fn policy_version(&self) -> u64 {
        self.qualifications.policy_version
    }

    /// 返回本次范围求值使用的组织修订；未读取组织范围时为 0，不访问数据库。
    pub fn organization_version(&self) -> u64 {
        self.scope.organization_version
    }

    /// 取得编译冻结的授权时点，以相同时间解释所有有效关系，不用墙钟重算条件。
    pub fn as_of(&self) -> Instant {
        self.context.as_of
    }

    /// 仓储执行前核对原执行器、事务、读取阶段及有效期，不读取数据库；结果交付前仍须调用 `AuthorizationService::validate`。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        self.validity.ensure_current(executor)
    }

    /// 获取身份、范围、查询模型与过滤阶段的稳定摘要，供跨页游标比较；不替代执行边界检查或交付复验。
    pub fn version(&self) -> String {
        self.version.clone()
    }

    /// 借用含最终授权过滤和临时字段清理的完整管道；先授权，再分页、计数或汇总，交付前仍须复验。
    pub fn stages(&self) -> &[Document] {
        &self.stages
    }

    /// 借用固定根模型，供仓储核对集合、记录身份和资源是否匹配；不得换用其他资源。
    pub fn model(&self) -> &QueryModel {
        &self.model
    }
}

impl fmt::Debug for CompiledAccess {
    // 诊断输出只包含模型描述，不打印账号身份或敏感业务事实。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("CompiledAccess").field("model", &self.model).finish_non_exhaustive()
    }
}
