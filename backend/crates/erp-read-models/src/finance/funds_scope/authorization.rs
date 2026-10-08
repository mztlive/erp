//! 资金授权解析与范围版本守卫。

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::ports::funds_scope::{FundsDataScopePort, FundsResolvedClause, FundsResolvedScope};
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
#[cfg(test)]
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use erp_procurement::{PurchaseAccess, PurchaseResolvedScope};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use super::rows::*;
use crate::{Error, Result};

/// 资金自身动作资格与真实来源边界；整账职责不替代任何来源判定。
pub struct FundsAuthorization {
    /// 关联销售单的已解析范围。
    pub sales: SalesReadScope,
    /// 明确的财务整账读取职责，不授予来源业务动作。
    pub ledger_read: bool,
    /// 结算来源范围，必须按实际来源类型选择。
    pub settlement: Option<AuthorizedDataScope>,
    /// 采购关联资源的已解析采购范围；销售关联查询为 `None`。
    pub purchase_scope: Option<PurchaseReadScope>,
    /// 资金资源动作的解析上下文。
    pub context: AuthorizedDataScope,
    pub(super) no_scope: bool,
}

impl FundsAuthorization {
    /// 判断销售、采购、结算与整账职责是否都没有可见规则。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 没有整账读取，且销售范围为空、采购范围缺失或为空、结算角色条款为空时返回 true。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn empty(&self) -> bool {
        !self.ledger_read
            && self.sales.is_empty()
            && self.purchase_scope.as_ref().is_none_or(PurchaseReadScope::is_empty)
            && self.settlement.as_ref().is_none_or(|scope| scope.scope.role_clauses.is_empty())
    }
}

/// 资金事实的关联责任：关联单据负责人、业务组织与经办人。
#[derive(Debug, Clone, Default)]
pub struct FundsLinkedFacts {
    /// 关联销售单或采购单的当前负责人。
    pub owner_user_id: Option<String>,
    /// 关联销售单或采购单的当前业务组织。
    pub business_org_unit_id: Option<String>,
    /// 经办人：回款登记人、核销提交人、发票创建人、付款提交人。
    pub operator_user_ids: Vec<String>,
    /// 第二经办人维度：开票申请的申请人（第一维度为当前处理人）。
    pub secondary_operator_user_ids: Vec<String>,
    /// 关联销售单或采购单主键。
    pub linked_document_id: String,
    /// 关联单据的业务版本。
    pub linked_document_version: u64,
}

/// 资金列表与汇总共用的授权快照元信息。
#[derive(Debug, Serialize)]
pub struct FundsScopedResult<T> {
    /// 业务数据。
    #[serde(flatten)]
    pub data: T,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 无范围时的空结果原因。
    pub empty_reason: Option<&'static str>,
    /// 面向客户端的范围摘要。
    pub scope_summary: &'static str,
    /// 归属口径说明。
    pub ownership_basis: &'static str,
}

/// 按关联责任把已解析范围编译为关联单据 ID 条件。
#[derive(Debug, Clone, Default)]
pub struct FundsLinkedCondition {
    /// 负责人精确身份条件。
    pub owner_user_ids: Option<Vec<String>>,
    /// 经办人精确身份条件。
    pub operator_user_ids: Option<Vec<String>>,
    /// 第二经办人精确身份条件；与第一经办人条件取交集。
    pub secondary_operator_user_ids: Option<Vec<String>>,
    /// 业务组织精确条件。
    pub org_unit_ids: Option<Vec<String>>,
}

/// 资金读取访问器；本域对象规则必须复用，不得与领域 Service 各维护一份。
#[derive(Clone)]
pub struct FundsAccess {
    pub(super) db: Database,
    pub(super) rbac: erp_identity::SharedRbacService,
    pub(super) scope: std::sync::Arc<dyn FundsDataScopePort>,
}

impl FundsAccess {
    /// 绑定资金集合数据库、身份服务与组合层注入的资金范围 Port。
    ///
    /// # 参数
    /// * `db` - 资金集合所在数据库
    /// * `rbac` - 当前 RBAC 快照服务
    /// * `scope` - 组合层注入的资金范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织；不得直接构造身份域 Service。
    pub fn new(
        db: Database,
        rbac: erp_identity::SharedRbacService,
        scope: std::sync::Arc<dyn FundsDataScopePort>,
    ) -> Self {
        Self { db, rbac, scope }
    }

    /// 在调用方事务内证明资金资源动作并映射关联销售范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `resource` - 本次解析的资金资源
    /// * `action` - 该资源已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回目标资金动作及来源销售范围；不要求额外销售操作权限。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配或未注册时拒绝。
    ///
    /// # 关键业务约束
    /// 授权、业务归属读取必须沿用调用方执行器；adapter 不得另开事务。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(FundsResolvedScope, FundsAuthorization)> {
        self.resolve_sources(actor, resource, action, false, false, executor).await
    }

    /// 证明资金自身动作后，按采购及结算的实际来源边界解析。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `resource` - 本次解析的资金资源。
    /// * `action` - 该资源已注册动作。
    /// * `_purchase_access` - 调用方传入的采购访问器；本方法不读取它。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回与实际来源绑定的授权，不读取镜像资金范围。
    ///
    /// # 错误
    /// 账号、目标动作或来源配置失效时拒绝。
    pub async fn resolve_with_purchase(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        _purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<(FundsResolvedScope, FundsAuthorization)> {
        self.resolve_sources(actor, resource, action, true, false, executor).await
    }

    /// 以关联销售责任事实复用公共单对象判定。
    ///
    /// # 参数
    /// * `access` - 本动作公共解析上下文
    /// * `facts` - 本域提供的关联责任与经办事实
    ///
    /// # 返回
    /// 返回角色范围和个人上限共同允许的判定。
    ///
    /// # 错误
    /// 未注册资源动作拒绝。
    pub fn allows(access: &FundsResolvedScope, facts: &FundsLinkedFacts) -> Result<bool> {
        let consumer = registration(&access.resource, &access.action)?;
        let resolved = ResolvedScope {
            role_clauses: access.role_clauses.iter().map(public_clause).collect(),
            user_limit: access.user_limit.as_ref().map(public_clause),
        };
        Ok(resolved.allows(
            &ScopedObject {
                owned: facts.owner_is(access.user_id.as_str()),
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: facts.business_org_unit_id.as_deref(),
                settlement_party_id: None,
                warehouse_id: None,
            },
            consumer.allows_history,
        ))
    }

    /// 以关联采购责任事实复用公共单对象判定。
    ///
    /// # 参数
    /// * `access` - 采购资源动作的已解析事实
    /// * `facts` - 关联采购责任与业务组织事实
    ///
    /// # 返回
    /// 返回角色范围和个人上限共同允许的判定。
    ///
    /// # 错误
    /// 资源或动作尚未接入 DataScope 时返回校验错误。
    pub fn allows_purchase(access: &PurchaseResolvedScope, facts: &FundsLinkedFacts) -> Result<bool> {
        let consumer = registration(&access.resource, &access.action)?;
        let resolved = ResolvedScope {
            role_clauses: access.role_clauses.iter().map(purchase_clause).collect(),
            user_limit: access.user_limit.as_ref().map(purchase_clause),
        };
        Ok(resolved.allows(
            &ScopedObject {
                owned: facts.owner_is(access.user_id.as_str()),
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: facts.business_org_unit_id.as_deref(),
                settlement_party_id: None,
                warehouse_id: None,
            },
            consumer.allows_history,
        ))
    }

    /// 将已证明范围编译为关联销售单 ID 限制。
    ///
    /// # 参数
    /// * `authorization` - 已证明的资金授权
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// `None` 表示公司范围不限制关联单；`Some` 为必须命中的关联单集合。
    ///
    /// # 错误
    /// 销售单 ID 查询失败时返回对应错误；结果超过 10000 条时返回校验错误并整体拒绝。
    ///
    /// # 关键业务约束
    /// 空集表示无可见对象；超限必须整体拒绝，不得截断汇总。
    pub async fn authorized_sales_ids(
        &self,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        if authorization.empty() {
            return Ok(Some(Vec::new()));
        }
        if authorization.sales.is_company() {
            return Ok(None);
        }
        let ids = self.db.sales_orders().list_authorized_ids(&authorization.sales, executor).await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("资金查询超过上限，请收窄组织或负责人条件".into()));
        }
        Ok(Some(ids))
    }

    /// 将已证明范围编译为关联采购单 ID 限制。
    ///
    /// # 参数
    /// * `purchase_access` - 组合层注入的采购访问器
    /// * `scope` - 已证明的采购对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// `None` 表示公司范围不限制关联单；`Some` 为必须命中的关联单集合。
    ///
    /// # 错误
    /// 采购来源 ID 解析失败时返回对应错误，其中包括超过查询上限。
    pub async fn authorized_purchase_ids(
        &self,
        purchase_access: &PurchaseAccess,
        scope: &erp_procurement::repository::purchase_order::scope::PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        purchase_access.authorized_source_ids(scope, executor).await.map_err(Error::from)
    }

    /// 展开请求中的内部组织及其可选有效下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织或组织树非法时拒绝。
    ///
    /// # 关键业务约束
    /// 筛选只能收窄授权结果，不得忽略未知组织；停用分支不贡献范围。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await.map_err(Error::from)
    }
}

/// 把资金 Port 事实无损转回公共判定输入，不读取或重解释原始规则。
///
/// # 参数
/// * `clause` - 已解析的资金范围条款。
///
/// # 返回
/// 复制公司、本人、协作与组织字段后的公共 `ScopeClause`；其余字段保持默认。
///
/// # 错误
/// 不返回错误。
pub(super) fn public_clause(clause: &FundsResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 把采购 Port 事实无损转回公共判定输入；协作不映射为采购对象。
///
/// # 参数
/// * `clause` - 已解析的采购范围条款。
///
/// # 返回
/// 复制公司、本人与组织字段的公共 `ScopeClause`；`collaborative` 固定为 false。
///
/// # 错误
/// 不返回错误。
pub(super) fn purchase_clause(clause: &erp_procurement::ports::PurchaseResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: false,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 由版本号构造组织版本快照；不读取组织集合。
#[cfg(test)]
pub(super) fn organization_version(version: u64) -> OrganizationState {
    OrganizationState { version, ..Default::default() }
}

impl FundsLinkedFacts {
    /// 判断当前操作人是否为关联单据的当前负责人。
    ///
    /// # 参数
    /// * `user` - 待比较的用户 ID。
    ///
    /// # 返回
    /// `owner_user_id` 等于 `user` 时返回 true；负责人为空时返回 false。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn owner_is(&self, user: &str) -> bool {
        self.owner_user_id.as_deref() == Some(user)
    }

    /// 返回关联单据业务版本，用于跨页版本绑定。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `{linked_document_id}:{linked_document_version}`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn version_part(&self) -> String {
        format!("{}:{}", self.linked_document_id, self.linked_document_version)
    }
}

/// 同字段 OR、不同字段 AND 的关联责任筛选。
///
/// # 参数
/// * `facts` - 关联责任事实
/// * `condition` - 请求中的负责人、经办人与组织条件
///
/// # 返回
/// 全部已提供条件命中时为 true；未提供的维度不约束。
///
/// # 错误
/// 无。
pub fn matches_linked_condition(facts: &FundsLinkedFacts, condition: &FundsLinkedCondition) -> bool {
    if let Some(owners) = &condition.owner_user_ids
        && !facts.owner_user_id.as_ref().is_some_and(|owner| owners.iter().any(|id| id == owner))
    {
        return false;
    }
    if let Some(operators) = &condition.operator_user_ids
        && !facts.operator_user_ids.iter().any(|operator| operators.iter().any(|id| id == operator))
    {
        return false;
    }
    if let Some(operators) = &condition.secondary_operator_user_ids
        && !facts.secondary_operator_user_ids.iter().any(|operator| operators.iter().any(|id| id == operator))
    {
        return false;
    }
    if let Some(orgs) = &condition.org_unit_ids
        && !facts.business_org_unit_id.as_ref().is_some_and(|org| orgs.iter().any(|id| id == org))
    {
        return false;
    }
    true
}

/// 同一授权条件下的人员汇总只计算匹配份额；未匹配份额单列。
///
/// # 参数
/// * `allocations` - 已按授权裁剪的分配份额
/// * `owner_of` - 分配归属负责人的解析函数
///
/// # 返回
/// 返回按负责人分组的份额合计与未分配余额；金额方向保持原事实正反。
///
/// # 错误
/// 不返回错误。金额相加使用 `Amount::checked_add`，本函数始终返回 `Ok`。
pub fn summarize_matched_shares(
    allocations: &[(String, Amount, Option<String>)],
    owner_of: impl Fn(&str) -> Option<String>,
) -> Result<(BTreeMap<String, Amount>, Amount)> {
    let mut grouped = BTreeMap::<String, Amount>::new();
    let mut unassigned = erp_finance::service::receivable::mapping::zero_amount();
    for (allocation_id, amount, order_id) in allocations {
        let owner = order_id.as_deref().and_then(&owner_of);
        match owner {
            Some(owner) => {
                let total = grouped
                    .remove(&owner)
                    .unwrap_or_else(erp_finance::service::receivable::mapping::zero_amount);
                grouped.insert(owner, total.checked_add(*amount));
            },
            None => {
                let _ = allocation_id;
                unassigned = unassigned.checked_add(*amount);
            },
        }
    }
    Ok((grouped, unassigned))
}

/// 首页面后必须携带同一范围版本，禁止不同授权页拼接。
///
/// # 参数
/// * `page` - 请求页码，从 1 起。
/// * `version` - 调用方回传的范围版本。
///
/// # 返回
/// 首页，或后续页携带非空版本时成功。
///
/// # 错误
/// 后续页缺少范围版本或版本为空时返回范围变化冲突错误。
pub fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    crate::support::ensure_deep_page(page, version)
}

/// 版本不一致时返回可识别的范围变化错误并要求从第一页刷新。
///
/// # 参数
/// * `expected` - 调用方回传的范围版本；`None` 表示不比对。
/// * `actual` - 本次快照计算出的范围版本。
///
/// # 返回
/// `expected` 为 `None` 或与 `actual` 相同时成功。
///
/// # 错误
/// `expected` 有值且不等于 `actual` 时返回范围变化冲突错误。
pub fn ensure_version(expected: Option<&str>, actual: &str) -> Result<()> {
    if expected.is_some_and(|version| version != actual) {
        return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
    }
    Ok(())
}

/// 把授权与关联单据版本绑定为跨页凭据；不返回内部授权集合。
///
/// # 参数
/// * `context` - 已解析的授权上下文，只取其 `scope_version`。
/// * `parts` - 追加进指纹的关联单据版本片段。
///
/// # 返回
/// 授权版本与 `parts` 的十六进制哈希。
///
/// # 错误
/// 不返回错误。
pub fn scope_version(context: &AuthorizedDataScope, parts: &[String]) -> String {
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    context.scope_version.hash(&mut fingerprint);
    for part in parts {
        part.hash(&mut fingerprint);
    }
    format!("{:x}", fingerprint.finish())
}

/// 部分授权的受限金额统一为空并注明权限限制，不得写零或差额推导。
///
/// # 参数
/// 无。
///
/// # 返回
/// 始终返回 `None`。
///
/// # 错误
/// 不返回错误。
pub fn restricted<T>() -> Option<T> {
    None
}

/// 整单读取才返回完整金额；部分授权统一返 null，禁止零值掩盖或差额推导。
///
/// # 参数
/// * `whole` - 是否具备整单读取资格。
/// * `amount` - 整单金额。
///
/// # 返回
/// `whole` 为 true 时返回 `Some(amount)`，否则返回 `None`。
///
/// # 错误
/// 不返回错误。
pub fn whole_amount(whole: bool, amount: Amount) -> Option<Amount> {
    whole.then_some(amount)
}
/// 空范围保持空集且版本可跨页回传，不得补公司范围。
///
/// # 参数
/// * `authorization` - 已解析授权，用于回传范围版本与策略版本。
/// * `reason` - 空结果原因。
/// * `summary` - 面向客户端的范围摘要。
///
/// # 返回
/// 第 1 页、页大小 20、总数 0 的空页；汇总标记权限受限且整单金额为空。
///
/// # 错误
/// 不返回错误。
pub(super) fn empty_page<T>(
    authorization: &FundsAuthorization,
    reason: &'static str,
    summary: &'static str,
) -> FundsScopedPage<T> {
    FundsScopedPage {
        items: Vec::new(),
        total: 0,
        summary: FundsSummaryView {
            grouped: Vec::new(),
            unassigned: erp_finance::service::receivable::mapping::zero_amount(),
            whole_total: None,
            permission_limited: true,
            scope_version: authorization.context.scope_version.clone(),
        },
        page: 1,
        page_size: 20,
        scope_version: authorization.context.scope_version.clone(),
        policy_version: authorization.context.policy_version,
        organization_version: authorization.context.organizations.version,
        as_of: authorization.context.as_of.as_utc().to_rfc3339(),
        empty_reason: Some(reason),
        scope_summary: summary,
        ownership_basis: "current_linked_responsibility",
    }
}

/// 连拍两次范围快照：先对照调用方 `expected`，再对照二次快照的 `scope_version`。
///
/// 必须保持该两段比较顺序，禁止合成一次读取。版本一致时交付第一次快照。
///
/// # 参数
/// * `expected` - 调用方回传的范围版本；首页可为 `None`
/// * `snapshot` - 产生范围页的闭包，按顺序调用两次
///
/// # 返回
/// 两次版本一致时返回第一次快照。
///
/// # 错误
/// * `ConflictError` - `expected` 与第一次快照不一致，或两次快照的 `scope_version` 不一致
pub(super) async fn checked_twice<T, F, Fut>(
    expected: Option<&str>,
    snapshot: F,
) -> Result<FundsScopedPage<T>>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<FundsScopedPage<T>>>,
{
    let first = snapshot().await?;
    if expected.is_some_and(|value| value != first.scope_version) {
        return Err(changed());
    }
    let current = snapshot().await?;
    if current.scope_version != first.scope_version {
        return Err(changed());
    }
    Ok(first)
}

/// 首次返回页面，第二次只重新解析资格并读取完整匹配集合的版本材料。
///
/// # 参数
/// * `expected` - 调用方回传的范围版本；首页可为 `None`。
/// * `snapshot` - 产生第一次范围页的闭包，只调用一次。
/// * `revalidate` - 独立重读范围版本的闭包；首次版本失配时不调用。
///
/// # 返回
/// 两段版本比较都通过时交付第一次页面。
///
/// # 错误
/// 调用方版本失配或第二轮授权/业务版本变化时返回范围变化冲突错误；首次失配不执行重验。
pub(super) async fn checked_revalidated<T, F, Fut, V, VersionFut>(
    expected: Option<&str>,
    snapshot: F,
    revalidate: V,
) -> Result<FundsScopedPage<T>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<FundsScopedPage<T>>>,
    V: FnOnce() -> VersionFut,
    VersionFut: Future<Output = Result<String>>,
{
    let first = snapshot().await?;
    ensure_version(expected, &first.scope_version)?;
    if revalidate().await? != first.scope_version {
        return Err(changed());
    }
    Ok(first)
}

/// 版本不一致时返回可识别的范围变化错误并要求从第一页刷新。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回说明为「数据范围已变化，请从第一页刷新」的范围变化冲突错误。
///
/// # 错误
/// 不返回错误。
pub(super) fn changed() -> Error {
    crate::support::data_scope_changed("数据范围已变化，请从第一页刷新")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub(super) fn linked_conditions_use_or_within_a_field_and_and_across_fields() {
        let facts = FundsLinkedFacts {
            owner_user_id: Some("sales-a".into()),
            business_org_unit_id: Some("org-a".into()),
            operator_user_ids: vec!["op-a".into()],
            secondary_operator_user_ids: vec!["applicant-a".into()],
            linked_document_id: "so-1".into(),
            linked_document_version: 3,
        };
        let matched = FundsLinkedCondition {
            owner_user_ids: Some(vec!["sales-b".into(), "sales-a".into()]),
            operator_user_ids: Some(vec!["op-a".into()]),
            secondary_operator_user_ids: Some(vec!["applicant-a".into()]),
            org_unit_ids: Some(vec!["org-a".into()]),
        };
        assert!(matches_linked_condition(&facts, &matched));
        let mismatched = FundsLinkedCondition {
            owner_user_ids: Some(vec!["sales-b".into()]),
            operator_user_ids: None,
            secondary_operator_user_ids: None,
            org_unit_ids: None,
        };
        assert!(!matches_linked_condition(&facts, &mismatched));
        assert!(matches_linked_condition(&facts, &FundsLinkedCondition::default()));
    }

    #[test]
    pub(super) fn matched_shares_group_by_owner_and_keep_unassigned_separate() {
        let allocations = [
            ("a1".to_string(), "60".parse().unwrap(), Some("so-1".to_string())),
            ("a2".to_string(), "40".parse().unwrap(), Some("so-2".to_string())),
            ("a3".to_string(), "10".parse().unwrap(), None),
        ];
        let owners = [("so-1".to_string(), "zhang".to_string()), ("so-2".to_string(), "li".to_string())]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        let (grouped, unassigned) =
            summarize_matched_shares(&allocations, |order| owners.get(order).cloned()).unwrap();
        assert_eq!(grouped["zhang"], "60".parse().unwrap());
        assert_eq!(grouped["li"], "40".parse().unwrap());
        assert_eq!(unassigned, "10".parse().unwrap());
    }

    #[test]
    pub(super) fn later_pages_require_version_and_changed_versions_are_rejected() {
        assert!(ensure_page(1, None).is_ok());
        assert!(ensure_page(2, None).is_err());
        assert!(ensure_page(2, Some("v1")).is_ok());
        assert!(ensure_version(Some("v1"), "v1").is_ok());
        assert!(ensure_version(Some("v1"), "v2").is_err());
    }

    fn test_page(scope_version: &str, page: u64) -> FundsScopedPage<()> {
        FundsScopedPage {
            items: Vec::new(),
            total: 0,
            summary: FundsSummaryView {
                grouped: Vec::new(),
                unassigned: erp_finance::service::receivable::mapping::zero_amount(),
                whole_total: None,
                permission_limited: true,
                scope_version: scope_version.to_string(),
            },
            page,
            page_size: 20,
            scope_version: scope_version.to_string(),
            policy_version: 0,
            organization_version: 0,
            as_of: String::new(),
            empty_reason: None,
            scope_summary: "",
            ownership_basis: "",
        }
    }

    #[tokio::test]
    async fn checked_twice_compares_expected_before_second_snapshot() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        let err = checked_twice(Some("old"), || {
            calls.set(calls.get() + 1);
            async { Ok(test_page("new", 1)) }
        })
        .await
        .unwrap_err();
        assert_eq!(calls.get(), 1);
        assert!(
            matches!(err, Error::ConflictError(message) if message == "DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新")
        );
    }

    #[tokio::test]
    async fn checked_twice_rejects_second_snapshot_change_and_returns_first_page() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        let err = checked_twice(Some("v1"), || {
            let n = calls.get();
            calls.set(n + 1);
            let version = if n == 0 { "v1" } else { "v2" };
            async move { Ok(test_page(version, 1)) }
        })
        .await
        .unwrap_err();
        assert_eq!(calls.get(), 2);
        assert!(matches!(err, Error::ConflictError(_)));

        let calls = Cell::new(0);
        let page = checked_twice(Some("v1"), || {
            let n = calls.get();
            calls.set(n + 1);
            async move { Ok(test_page("v1", n as u64 + 1)) }
        })
        .await
        .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(page.page, 1);
        assert_eq!(page.scope_version, "v1");
    }

    /// 初次失配立即失败；完整集合重验变化不得返回首次页面。
    #[tokio::test]
    async fn lightweight_revalidation_preserves_comparison_order_and_first_page() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        let result = checked_revalidated(
            Some("old"),
            || async { Ok(test_page("new", 7)) },
            || {
                calls.set(calls.get() + 1);
                async { Ok("new".into()) }
            },
        )
        .await;
        assert!(matches!(result, Err(Error::ConflictError(_))));
        assert_eq!(calls.get(), 0);
        let changed = checked_revalidated(
            Some("new"),
            || async { Ok(test_page("new", 7)) },
            || async { Ok("changed-outside-page".into()) },
        )
        .await;
        assert!(matches!(changed, Err(Error::ConflictError(_))));
        let page = checked_revalidated(
            Some("new"),
            || async { Ok(test_page("new", 7)) },
            || async { Ok("new".into()) },
        )
        .await
        .unwrap();
        assert_eq!(page.page, 7);
    }

    #[test]
    pub(super) fn scope_versions_bind_authorization_and_linked_documents() {
        let context = AuthorizedDataScope {
            user_id: "u1".into(),
            resource: "customer_receipt".into(),
            action: "list".into(),
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
            role_scopes: Default::default(),
            organizations: organization_version(7),
            policy_version: 1,
            scope_version: "base".into(),
            as_of: erp_core::common::time::Instant::from_unix_secs(0),
        };
        let first = scope_version(&context, &["so-1:3".to_string()]);
        let second = scope_version(&context, &["so-1:4".to_string()]);
        assert_ne!(first, second);
    }
}
