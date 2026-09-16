//! 资金往来共享授权映射：同一对象映射供列表、详情、汇总、候选、导出、命令复用。
//!
//! 按主合同第 7 章执行金额裁剪：人员汇总只算匹配份额；部分授权只返获授权份额，
//! 整单金额、整单已分配合计、未分配余额、其他分配及完整凭证字段返 null。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use application_core::{AuditActor, FilterOption};
use erp_audit::AuditExt;
use erp_core::money::Amount;
use erp_finance::ports::funds_scope::{FundsDataScopePort, FundsResolvedClause, FundsResolvedScope};
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_identity::AccessControlExt;
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
use erp_identity::entity::organization_change::OrganizationState;
use erp_identity::service::access_control::consumers::registration;
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use erp_procurement::{PurchaseAccess, PurchaseResolvedScope};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use erp_workflow::WorkItemExt;
use erp_workflow::repository::ApprovalIntegrationExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;
use validator::Validate;

use crate::sales_center::access::{SalesAccess, sales_scope};
use crate::{Error, Result};

/// 资金读取动作共用的已解析授权：关联销售范围与资金资源范围求交。
pub struct FundsAuthorization {
    /// 关联销售单的已解析范围。
    pub sales: SalesReadScope,
    /// 资金资源的已解析范围。
    pub funds: SalesReadScope,
    /// 采购关联资源的已解析采购范围；销售关联查询为 `None`。
    pub purchase_scope: Option<PurchaseReadScope>,
    /// 资金资源动作的解析上下文。
    pub context: AuthorizedDataScope,
    fingerprint: std::collections::hash_map::DefaultHasher,
    no_scope: bool,
}

impl FundsAuthorization {
    /// 两组独立范围同时为公司范围时才有整单读取资格。
    pub fn whole(&self) -> bool {
        self.sales.is_company() && self.funds.is_company()
    }

    /// 任一组范围为空即无可见对象。
    pub fn empty(&self) -> bool {
        self.sales.is_empty() || self.funds.is_empty()
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
    db: Database,
    rbac: erp_identity::SharedRbacService,
    scope: std::sync::Arc<dyn FundsDataScopePort>,
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
    /// 返回已解析的资金授权；关联销售读取动作同时由同角色证明。
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
        let access = self.scope.resolve(actor, resource, action, executor).await.map_err(Error::from)?;
        let (sales_access, sales) =
            self.linked_sales_scope(actor, &access, executor).await.map_err(Error::from)?;
        let mut authorization = FundsAuthorization {
            sales,
            funds: sales_scope(&access_output(&access)?, actor.id(), &[], Vec::new()),
            purchase_scope: None,
            context: access_output(&access)?,
            fingerprint: Default::default(),
            no_scope: false,
        };
        sales_access.scope_version.hash(&mut authorization.fingerprint);
        authorization.context.scope_version.hash(&mut authorization.fingerprint);
        authorization.no_scope = authorization.empty();
        Ok((access, authorization))
    }

    /// 在调用方事务内证明采购关联资金资源的动作。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `resource` - 本次解析的资金资源
    /// * `action` - 该资源已注册动作
    /// * `purchase_access` - 组合层注入的采购访问器
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析的资金授权；关联采购读取动作同时由同角色证明。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配或未注册时拒绝。
    pub async fn resolve_with_purchase(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<(FundsResolvedScope, FundsAuthorization)> {
        let access = self.scope.resolve(actor, resource, action, executor).await.map_err(Error::from)?;
        let (purchase_resolved, purchase_scope) =
            self.linked_purchase_scope(actor, purchase_access, executor).await.map_err(Error::from)?;
        let mut authorization = FundsAuthorization {
            sales: SalesReadScope {
                required_scopes: vec![],
                roles: purchase_scope
                    .roles
                    .iter()
                    .map(|clause| erp_sales::repository::sales_order::scope::SalesScopeClause {
                        company: clause.company,
                        owner_user_id: clause.owner_user_id.clone(),
                        business_org_unit_ids: clause.business_org_unit_ids.clone(),
                        collaborative_customer_ids: Vec::new(),
                    })
                    .collect(),
                user_limit: purchase_scope.user_limit.as_ref().map(|clause| {
                    erp_sales::repository::sales_order::scope::SalesScopeClause {
                        company: clause.company,
                        owner_user_id: clause.owner_user_id.clone(),
                        business_org_unit_ids: clause.business_org_unit_ids.clone(),
                        collaborative_customer_ids: Vec::new(),
                    }
                }),
                historical_order_ids: purchase_scope.historical_order_ids.clone(),
            },
            funds: sales_scope(&access_output(&access)?, actor.id(), &[], Vec::new()),
            purchase_scope: Some(purchase_scope),
            context: access_output(&access)?,
            fingerprint: Default::default(),
            no_scope: false,
        };
        purchase_resolved.scope_version.hash(&mut authorization.fingerprint);
        authorization.context.scope_version.hash(&mut authorization.fingerprint);
        authorization.no_scope = authorization.empty();
        Ok((access, authorization))
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
                collaborating: facts.collaborating_is(access.user_id.as_str()),
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
    /// 责任事实损坏或授权 Port 未装配时拒绝。
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
    /// 超过查询上限时整体拒绝。
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
        if authorization.whole() {
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
    /// 超过查询上限时整体拒绝。
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

    /// 在调用方事务内证明关联销售读取动作，由同角色同时持有资金动作。
    async fn linked_sales_scope(
        &self,
        actor: &AuditActor,
        access: &FundsResolvedScope,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, SalesReadScope)> {
        let _ = access;
        let resolver = SalesAccess::new(self.db.clone(), self.rbac.clone());
        let permission = erp_identity::Permission::parse("sales_order:list")?;
        resolver.resolve(actor, "list", &[permission], executor).await
    }

    /// 在调用方事务内证明关联采购读取动作。
    async fn linked_purchase_scope(
        &self,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, erp_procurement::repository::purchase_order::scope::PurchaseReadScope)>
    {
        purchase_access.resolve(actor, "list", executor).await.map_err(Error::from)
    }
}

/// 把资金 Port 事实无损转回公共判定输入，不读取或重解释原始规则。
fn public_clause(clause: &FundsResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: clause.collaborative,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 把采购 Port 事实无损转回公共判定输入；协作不映射为采购对象。
fn purchase_clause(clause: &erp_procurement::ports::PurchaseResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: false,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 把资金 Port 事实转为销售条件编译可用的已解析上下文。
fn access_output(access: &FundsResolvedScope) -> Result<AuthorizedDataScope> {
    let consumer = registration(&access.resource, &access.action)?;
    let _ = consumer;
    Ok(AuthorizedDataScope {
        user_id: access.user_id.clone(),
        resource: access.resource.clone(),
        action: access.action.clone(),
        scope: ResolvedScope {
            role_clauses: access.role_clauses.iter().map(public_clause).collect(),
            user_limit: access.user_limit.as_ref().map(public_clause),
        },
        role_scopes: Default::default(),
        organizations: organization_version(access.organization_version),
        policy_version: access.policy_version,
        scope_version: access.scope_version.clone(),
        as_of: access.as_of,
    })
}

/// 由版本号构造组织版本快照；不读取组织集合。
fn organization_version(version: u64) -> OrganizationState {
    OrganizationState { version, ..Default::default() }
}

impl FundsLinkedFacts {
    /// 判断当前操作人是否为关联单据的当前负责人。
    fn owner_is(&self, user: &str) -> bool {
        self.owner_user_id.as_deref() == Some(user)
    }

    /// 判断当前操作人是否具备有效协作事实。
    fn collaborating_is(&self, _user: &str) -> bool {
        false
    }

    /// 返回关联单据业务版本，用于跨页版本绑定。
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
/// 金额合计溢出时拒绝。
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
                    .unwrap_or_else(|| erp_finance::service::receivable::mapping::zero_amount());
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
pub fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    if page > 1 && version.is_none_or(str::is_empty) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：请从第一页刷新后继续查询".into()));
    }
    Ok(())
}

/// 版本不一致时返回可识别的范围变化错误并要求从第一页刷新。
pub fn ensure_version(expected: Option<&str>, actual: &str) -> Result<()> {
    if expected.is_some_and(|version| version != actual) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into()));
    }
    Ok(())
}

/// 把授权与关联单据版本绑定为跨页凭据；不返回内部授权集合。
pub fn scope_version(context: &AuthorizedDataScope, parts: &[String]) -> String {
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    context.scope_version.hash(&mut fingerprint);
    for part in parts {
        part.hash(&mut fingerprint);
    }
    format!("{:x}", fingerprint.finish())
}

/// 部分授权的受限金额统一为空并注明权限限制，不得写零或差额推导。
pub fn restricted<T>() -> Option<T> {
    None
}

/// 整单读取才返回完整金额；部分授权统一返 null，禁止零值掩盖或差额推导。
pub fn whole_amount(whole: bool, amount: Amount) -> Option<Amount> {
    whole.then_some(amount)
}

/// M07/M08/M09 列表共用的范围分页视图；跨页必须原样回传 `scope_version`。
#[derive(Debug, Clone, Serialize)]
pub struct FundsScopedPage<T> {
    /// 本页已按同一对象映射裁剪的业务行。
    pub items: Vec<T>,
    /// 符合授权与业务条件的总数（截断前计数，超限整体拒绝）。
    pub total: u64,
    /// 同一快照下按负责人分组的匹配份额汇总；与列表复用同一授权条件和金额口径。
    pub summary: FundsSummaryView,
    /// 同一授权边界内的负责人候选；可区分离线停用人员，不得用于改派。
    pub owner_options: Vec<FilterOption>,
    /// 当前页码。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
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

/// M07 应收往来子账范围行：部分授权只返获授权核销份额，整单金额为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedReceivableAccountRow {
    /// 子账主键。
    pub id: String,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 子账状态。
    pub status: erp_finance::entity::receivable::ReceivableAccountStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权核销份额合计（匹配份额求和，60/40 不重复记）。
    pub visible_settled_share: Amount,
    /// 整单含税应收总额；部分授权为 null。
    pub gross_total: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub settled_total: Option<Amount>,
    /// 未分配余额；按资金单据自身规则判定，部分授权为 null。
    pub open_total: Option<Amount>,
    /// 部分受限时为 true，客户端必须提示权限限制与可见金额。
    pub permission_limited: bool,
    /// 关联销售单当前负责人。
    pub sales_owner_user_id: Option<String>,
    /// 关联销售单当前业务组织。
    pub business_org_unit_id: Option<String>,
}

/// M07 客户回款范围行：登记/核销经办人分别查询，整单与未分配部分授权为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedCustomerReceiptRow {
    /// 回款单主键。
    pub id: String,
    /// 回款单号。
    pub receipt_no: String,
    /// 回款单状态。
    pub status: erp_finance::entity::receivable::CustomerReceiptStatus,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: erp_core::common::time::Instant,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权核销份额合计。
    pub visible_allocated_share: Amount,
    /// 整单到账金额；部分授权为 null。
    pub amount: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub allocated_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub unallocated_amount: Option<Amount>,
    /// 获授权分配行；部分授权仅含获授权份额行，其他分配不返回。
    pub allocations: Option<Vec<erp_finance::dto::receivable::ReceiptAllocationView>>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// M08 销项发票范围行：整单金额与完整凭证部分授权为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedInvoiceRow {
    /// 发票主键。
    pub id: String,
    /// 发票号码。
    pub invoice_no: String,
    /// 发票方向。
    pub invoice_direction: erp_finance::entity::receivable::InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: erp_finance::entity::receivable::InvoiceKind,
    /// 发票状态。
    pub status: erp_finance::entity::receivable::InvoiceStatus,
    /// 开票日期。
    pub invoice_date: erp_core::common::time::BusinessDate,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权分配份额合计。
    pub visible_allocated_share: Amount,
    /// 整单含税金额；部分授权为 null。
    pub gross_amount: Option<Amount>,
    /// 整单已分配合计；部分授权为 null。
    pub allocated_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub unallocated_amount: Option<Amount>,
    /// 获授权分配行；部分授权仅含获授权份额行。
    pub allocations: Option<Vec<erp_finance::dto::receivable::SalesInvoiceAllocationView>>,
    /// 进项方向获授权分配行；销项发票为空，部分授权仅含获授权份额行。
    pub purchase_allocations: Option<Vec<erp_finance::dto::payable::PurchaseInvoiceAllocationView>>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// M08 开票申请范围行：负责销售/申请人/当前开票处理人分别查询，不改变正式开票准入。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedInvoiceRequestRow {
    /// 申请主键。
    pub id: String,
    /// 申请单号。
    pub request_no: String,
    /// 关联销售单。
    pub sales_order_id: String,
    /// 关联销售单业务单号。
    pub sales_order_no: String,
    /// 申请状态。
    pub status: erp_finance::entity::receivable::InvoiceRequestStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 申请人（创建人）。
    pub applicant_user_id: String,
    /// 当前开票处理人（关联工作项当前负责人）；无工作项为空。
    pub handler_user_id: Option<String>,
    /// 申请金额；部分授权仍返回本行申请金额（行级事实），整单汇总为 null 由汇总视图承担。
    pub amount: Amount,
    /// 部分受限时为 true。
    pub permission_limited: bool,
    /// 关联销售单当前负责人。
    pub sales_owner_user_id: Option<String>,
    /// 关联销售单当前业务组织。
    pub business_org_unit_id: Option<String>,
}

/// M09 应付往来子账范围行：部分授权只返获授权份额，整单金额为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedPayableAccountRow {
    /// 子账主键。
    pub id: String,
    /// 来源单据。
    pub source_document_id: String,
    /// 来源类型。
    pub source_type: erp_finance::entity::payable::PayableSourceType,
    /// 往来供应商。
    pub supplier_id: String,
    /// 子账状态。
    pub status: erp_finance::entity::payable::PayableAccountStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权核销份额合计。
    pub visible_settled_share: Amount,
    /// 整单含税应付总额；部分授权为 null。
    pub gross_total: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub settled_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub open_total: Option<Amount>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
    /// 来源采购单当前采购负责人。
    pub procurement_owner_user_id: Option<String>,
    /// 来源采购单当前业务组织。
    pub business_org_unit_id: Option<String>,
}

/// M09 供应商付款范围行：采购负责人与付款经办人分别查询。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedSupplierPaymentRow {
    /// 付款单主键。
    pub id: String,
    /// 付款单号。
    pub payment_no: String,
    /// 付款单状态。
    pub status: erp_finance::entity::payable::SupplierPaymentStatus,
    /// 收款供应商。
    pub supplier_id: String,
    /// 实际付款时间（秒级时间戳）。
    pub paid_at: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权核销份额合计。
    pub visible_allocated_share: Amount,
    /// 整单付款金额；部分授权为 null。
    pub amount: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub allocated_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub unallocated_amount: Option<Amount>,
    /// 获授权分配行；部分授权仅含获授权份额行。
    pub allocations: Option<Vec<erp_finance::dto::payable::PaymentAllocationView>>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// M09 进项发票分配范围行：按应付子账来源采购责任过滤。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedPurchaseInvoiceAllocationRow {
    /// 分配主键。
    pub id: String,
    /// 进项发票。
    pub invoice_id: String,
    /// 进项发票号码。
    pub invoice_no: Option<String>,
    /// 应付子账。
    pub payable_account_id: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权分配金额。
    pub visible_allocated_amount: Amount,
    /// 整单分配金额；部分授权为 null（本行即份额时仍返 null，由可见份额承担）。
    pub allocated_gross_amount: Option<Amount>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// 人员汇总行：只算匹配份额，未分配单列，部分授权仅返获授权份额。
#[derive(Debug, Clone, Serialize)]
pub struct FundsPersonShare {
    /// 负责人。
    pub owner_user_id: String,
    /// 匹配份额合计。
    pub visible_share: Amount,
}

/// 汇总视图：分组份额与未分配余额；整单合计部分授权为 null。
#[derive(Debug, Clone, Serialize)]
pub struct FundsSummaryView {
    /// 按负责人分组的匹配份额。
    pub grouped: Vec<FundsPersonShare>,
    /// 未分配余额（无关联单据份额）。
    pub unassigned: Amount,
    /// 整单合计；部分授权为 null，禁止差额推导。
    pub whole_total: Option<Amount>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
}

/// 候选行：可区分离线授权候选与无资格对象；不可见对象不返回。
#[derive(Debug, Clone, Serialize)]
pub struct FundsCandidate {
    /// 候选主键。
    pub id: String,
    /// 展示标签。
    pub label: String,
    /// 关联负责人。
    pub owner_user_id: Option<String>,
    /// 部分受限原因；整单可见时为空。
    pub permission_limited_reason: Option<&'static str>,
}

/// M07 应收子账范围查询：负责销售与登记经办人分别查询，同一事务内解析与取数。
impl FundsAccess {
    /// 分页查询应收往来子账范围行。
    pub async fn receivable_account_list_scoped(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot =
            self.checked_receivable_accounts(params, &query, actor, params.scope_version.as_deref()).await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析应收子账详情动作；不可见与不存在统一为 NotFound。
    pub async fn receivable_account_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<FundsScopedResult<ScopedReceivableAccountRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (access, authorization) =
                        this.resolve(&actor, "receivable_account", "detail", executor).await?;
                    let account = this
                        .db
                        .receivable_accounts()
                        .find_by_id(&id, executor)
                        .await
                        .map_err(Error::from)?
                        .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
                    let order = this
                        .db
                        .sales_orders()
                        .find_by_id(&account.sales_order_id.to_string(), executor)
                        .await
                        .map_err(Error::from)?
                        .ok_or_else(|| Error::NotFound("应收账户来源销售单不存在".into()))?;
                    let facts = FundsLinkedFacts {
                        owner_user_id: Some(order.sales_owner_user_id.clone()),
                        business_org_unit_id: Some(order.business_org_unit_id.clone()),
                        operator_user_ids: vec![account.stable.created_by.clone()],
                        secondary_operator_user_ids: Vec::new(),
                        linked_document_id: order.base.id.clone(),
                        linked_document_version: order.base.version,
                    };
                    if !Self::allows(&access, &facts)? {
                        return Err(Error::NotFound("应收往来子账不存在".into()));
                    }
                    let whole = authorization.whole();
                    let visible = this.receivable_visible_share(&account.base.id, executor).await?;
                    let row = ScopedReceivableAccountRow {
                        id: account.base.id.clone(),
                        sales_order_id: account.sales_order_id.to_string(),
                        account_seq: account.account_seq,
                        status: account.stable.status(),
                        created_at: account.base.created_at,
                        visible_settled_share: visible,
                        gross_total: whole_amount(whole, account.gross_total),
                        settled_total: whole_amount(whole, account.settled_total),
                        open_total: whole_amount(whole, account.open_total),
                        permission_limited: !whole,
                        sales_owner_user_id: Some(order.sales_owner_user_id.clone()),
                        business_org_unit_id: Some(order.business_org_unit_id.clone()),
                    };
                    let parts = vec![facts.version_part()];
                    let version = scope_version(&authorization.context, &parts);
                    Ok(FundsScopedResult {
                        data: row,
                        scope_version: version,
                        policy_version: authorization.context.policy_version,
                        organization_version: authorization.context.organizations.version,
                        as_of: authorization.context.as_of.as_utc().to_rfc3339(),
                        empty_reason: None,
                        scope_summary: "应收子账按关联销售当前负责人与登记经办人授权；部分授权仅返获授权份额",
                        ownership_basis: "current_sales_owner_and_register_operator",
                    })
                })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    async fn checked_receivable_accounts(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        let snapshot = self.snapshot_receivable_accounts(params, query, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_receivable_accounts(params, query, actor).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot_receivable_accounts(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { this.load_receivable_accounts(&params, &query, &actor, executor).await },
                )
            })
            .await
    }

    /// 隐藏来源条件拒绝，空范围保持空集；完整装载并裁剪后才允许分页。
    async fn load_receivable_accounts(
        &self,
        params: &erp_finance::dto::receivable::ReceivableAccountListParams,
        query: &erp_finance::dto::receivable::ReceivableAccountListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedReceivableAccountRow>> {
        let (access, authorization) = self.resolve(actor, "receivable_account", "list", executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "应收子账无可见范围"));
        }
        let expanded_orgs = match (&query.org_unit_ids, query.include_descendants) {
            (Some(ids), _) => {
                let list = ids.as_slice().iter().cloned().collect::<Vec<_>>();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            (None, _) => None,
        };
        let authorized_ids = self.authorized_sales_ids(&authorization, executor).await?;
        let authorized_set = authorized_ids.map(|ids| ids.into_iter().collect::<BTreeSet<_>>());
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Receivable,
        )
        .await?;
        let filter = erp_finance::repository::ReceivableAccountFilter {
            keyword_ids,
            keyword: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_party_ids: Vec::new(),
            account_id: query.account_id.clone(),
            customer_id: query.customer_id.clone(),
            counterparty_party_id: query.counterparty_party_id.clone(),
            status: query.status,
            sales_order_id: query.sales_order_id.clone(),
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, erp_finance::dto::receivable::SortDir::Asc),
        };
        let candidates = self
            .db
            .receivable_accounts()
            .search_receivable_accounts(&filter, executor)
            .await
            .map_err(Error::from)?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("应收查询超过上限，请收窄组织或负责人条件".into()));
        }
        let sales_ids = candidates
            .items
            .iter()
            .map(|row| row.sales_order_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut orders_by_id = std::collections::HashMap::new();
        for chunk in sales_ids.chunks(500) {
            let ids = chunk.iter().map(erp_core::ids::SalesOrderId::new).collect::<Vec<_>>();
            for order in
                self.db.sales_orders().find_orders_by_ids(&ids, executor).await.map_err(Error::from)?
            {
                orders_by_id.insert(order.base.id.clone(), order);
            }
        }
        let condition = FundsLinkedCondition {
            owner_user_ids: query
                .sales_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            operator_user_ids: query
                .operator_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            secondary_operator_user_ids: None,
            org_unit_ids: expanded_orgs,
        };
        let mut filtered = Vec::new();
        for row in candidates.items {
            let Some(order) = orders_by_id.get(&row.sales_order_id) else {
                continue;
            };
            if let Some(allowed) = &authorized_set
                && !allowed.contains(&order.base.id)
            {
                continue;
            }
            let facts = FundsLinkedFacts {
                owner_user_id: Some(order.sales_owner_user_id.clone()),
                business_org_unit_id: Some(order.business_org_unit_id.clone()),
                operator_user_ids: vec![row.stable.created_by.clone()],
                secondary_operator_user_ids: Vec::new(),
                linked_document_id: order.base.id.clone(),
                linked_document_version: order.base.version,
            };
            if !Self::allows(&access, &facts)? {
                continue;
            }
            if !matches_linked_condition(&facts, &condition) {
                continue;
            }
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            order.base.id.hash(&mut fingerprint);
            order.base.version.hash(&mut fingerprint);
            filtered.push((row, order.base.id.clone(), order.base.version));
        }
        let total = filtered.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = query.paging.page_size.max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(filtered.len());
        let whole = authorization.whole();
        let mut items = Vec::new();
        if start < filtered.len() {
            for (row, _, _) in filtered[start..end].iter() {
                let visible = self.receivable_visible_share(&row.id, executor).await?;
                items.push(ScopedReceivableAccountRow {
                    id: row.id.clone(),
                    sales_order_id: row.sales_order_id.clone(),
                    account_seq: row.account_seq,
                    status: row.stable.status(),
                    created_at: row.created_at,
                    visible_settled_share: visible,
                    gross_total: whole_amount(whole, row.gross_total),
                    settled_total: whole_amount(whole, row.settled_total),
                    open_total: whole_amount(whole, row.open_total),
                    permission_limited: !whole,
                    sales_owner_user_id: orders_by_id
                        .get(&row.sales_order_id)
                        .map(|order| order.sales_owner_user_id.clone()),
                    business_org_unit_id: orders_by_id
                        .get(&row.sales_order_id)
                        .map(|order| order.business_org_unit_id.clone()),
                });
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        for (row, order_id, _) in filtered.iter() {
            let visible = self.receivable_visible_share(&row.id, executor).await?;
            triples.push((row.id.clone(), visible, Some(order_id.clone())));
            whole_sum = whole_sum.checked_add(row.gross_total);
        }
        let owner_of = orders_by_id
            .iter()
            .map(|(id, order)| (id.clone(), order.sales_owner_user_id.clone()))
            .collect::<HashMap<_, _>>();
        let summary = build_summary(&triples, &owner_of, whole_amount(whole, whole_sum), &version, !whole)?;
        let owner_options = self.owner_options_sales(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应收子账按关联销售当前负责人与登记经办人授权；部分授权仅返获授权份额",
            ownership_basis: "current_sales_owner_and_register_operator",
        })
    }

    /// 计算子账获授权核销份额；未分配与未授权份额不计入，禁止差额推导。
    async fn receivable_visible_share(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Amount> {
        use erp_core::ids::{ReceivableAccountId, ReceivableEntryId};
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_accounts(
                std::slice::from_ref(&ReceivableAccountId::new(account_id.to_string())),
                executor,
            )
            .await
            .map_err(Error::from)?;
        let entry_ids =
            entries.iter().map(|entry| ReceivableEntryId::new(entry.base.id.clone())).collect::<Vec<_>>();
        if entry_ids.is_empty() {
            return Ok(erp_finance::service::receivable::mapping::zero_amount());
        }
        let allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_entries(&entry_ids, executor)
            .await
            .map_err(Error::from)?;
        let mut net = erp_finance::service::receivable::mapping::zero_amount();
        for allocation in allocations {
            match allocation.allocation_action {
                erp_finance::entity::receivable::AllocationAction::Apply => {
                    net = net.checked_add(allocation.allocated_amount);
                },
                erp_finance::entity::receivable::AllocationAction::Reverse => {
                    net = net.checked_sub(allocation.allocated_amount);
                },
            }
        }
        Ok(net)
    }
}

/// 空范围保持空集且版本可跨页回传，不得补公司范围。
fn empty_page<T>(
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
        owner_options: Vec::new(),
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

/// 版本不一致时返回可识别的范围变化错误并要求从第一页刷新。
fn changed() -> Error {
    Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_conditions_use_or_within_a_field_and_and_across_fields() {
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
    fn matched_shares_group_by_owner_and_keep_unassigned_separate() {
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
    fn later_pages_require_version_and_changed_versions_are_rejected() {
        assert!(ensure_page(1, None).is_ok());
        assert!(ensure_page(2, None).is_err());
        assert!(ensure_page(2, Some("v1")).is_ok());
        assert!(ensure_version(Some("v1"), "v1").is_ok());
        assert!(ensure_version(Some("v1"), "v2").is_err());
    }

    #[test]
    fn scope_versions_bind_authorization_and_linked_documents() {
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

    #[test]
    fn whole_document_rules_require_both_funds_and_linked_company() {
        let authorization = FundsAuthorization {
            sales: erp_sales::repository::sales_order::scope::SalesReadScope {
                roles: vec![erp_sales::repository::sales_order::scope::SalesScopeClause {
                    company: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            funds: erp_sales::repository::sales_order::scope::SalesReadScope {
                roles: vec![erp_sales::repository::sales_order::scope::SalesScopeClause {
                    company: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            purchase_scope: None,
            context: AuthorizedDataScope {
                user_id: "u1".into(),
                resource: "customer_receipt".into(),
                action: "list".into(),
                scope: ResolvedScope { role_clauses: vec![], user_limit: None },
                role_scopes: Default::default(),
                organizations: organization_version(1),
                policy_version: 1,
                scope_version: "v".into(),
                as_of: erp_core::common::time::Instant::from_unix_secs(0),
            },
            fingerprint: Default::default(),
            no_scope: false,
        };
        assert!(authorization.whole());
        assert!(!authorization.empty());
    }
}

// ===== S3-03 M07/M08/M09 范围查询：关联责任映射与金额裁剪 =====
//
// 同一对象映射供列表、详情、汇总、候选、导出、命令复用；各入口按自身动作
// 独立解析，授权与业务读取沿用调用方同一执行器。

/// 关联销售单的当前责任事实。
#[derive(Debug, Clone)]
struct LinkedSalesFact {
    /// 当前负责销售。
    owner_user_id: String,
    /// 当前业务组织。
    business_org_unit_id: String,
    /// 单据业务版本。
    version: u64,
}

/// 关联采购单的当前责任事实；老单缺责任人时负责人为空。
#[derive(Debug, Clone)]
struct LinkedPurchaseFact {
    /// 当前采购负责人；缺失时仅公司范围可见。
    owner_user_id: Option<String>,
    /// 当前业务组织。
    business_org_unit_id: String,
    /// 单据业务版本。
    version: u64,
}

/// 单据某条分配归属的关联单据；`None` 表示无关联单据的未分配份额。
type LinkedOrderId = Option<String>;

/// 回款分配的授权裁剪单元：金额方向、归属订单与响应视图。
struct ReceiptLink {
    /// 分配主键。
    id: String,
    /// 正反动作后的记账方向金额。
    signed: Amount,
    /// 归属销售单；缺失时计入未分配。
    order: LinkedOrderId,
    /// 响应视图。
    view: erp_finance::dto::receivable::ReceiptAllocationView,
}

/// 销项发票分配的授权裁剪单元。
struct SalesInvoiceLink {
    /// 分配主键。
    id: String,
    /// 正反动作后的含税方向金额。
    signed: Amount,
    /// 归属销售单；缺失时计入未分配。
    order: LinkedOrderId,
    /// 响应视图。
    view: erp_finance::dto::receivable::SalesInvoiceAllocationView,
}

/// 付款核销的授权裁剪单元。
struct PaymentLink {
    /// 分配主键。
    id: String,
    /// 正反动作后的记账方向金额。
    signed: Amount,
    /// 归属采购单；结算单来源与缺失时计入未分配。
    order: LinkedOrderId,
    /// 响应视图（来源单号不回填，不得当单号展示）。
    view: erp_finance::dto::payable::PaymentAllocationView,
}

impl FundsAccess {
    /// 批量读取关联销售单当前责任；缺失单据按无责任处理，调用方跳过该行。
    async fn sales_fact_map(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, LinkedSalesFact>> {
        use erp_core::ids::SalesOrderId;
        let mut map = HashMap::new();
        let mut unique = ids.to_vec();
        unique.sort();
        unique.dedup();
        for chunk in unique.chunks(500) {
            let keys = chunk.iter().map(|id| SalesOrderId::new(id.clone())).collect::<Vec<_>>();
            for order in self.db.sales_orders().find_orders_by_ids(&keys, executor).await? {
                map.insert(
                    order.base.id.clone(),
                    LinkedSalesFact {
                        owner_user_id: order.sales_owner_user_id.clone(),
                        business_org_unit_id: order.business_org_unit_id.clone(),
                        version: order.base.version,
                    },
                );
            }
        }
        Ok(map)
    }

    /// 批量读取关联采购单当前责任；责任人缺失的老单保留组织事实。
    async fn purchase_fact_map(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, LinkedPurchaseFact>> {
        let mut map = HashMap::new();
        let mut unique = ids.to_vec();
        unique.sort();
        unique.dedup();
        for chunk in unique.chunks(500) {
            let keys = chunk.to_vec();
            for order in self.db.purchase_order().find_orders_by_ids(&keys, executor).await? {
                map.insert(
                    order.base.id.clone(),
                    LinkedPurchaseFact {
                        owner_user_id: order.current_owner_user_id().ok().map(str::to_string),
                        business_org_unit_id: order.business_org_unit_id.clone(),
                        version: order.base.version,
                    },
                );
            }
        }
        Ok(map)
    }

    /// 同一授权边界内的负责销售候选；超过上限整体拒绝，不得截断。
    async fn owner_options_sales(
        &self,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FilterOption>> {
        if authorization.empty() {
            return Ok(Vec::new());
        }
        let ids = self.db.sales_orders().current_owner_ids(&authorization.sales, executor).await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("负责人候选超过查询上限".into()));
        }
        Ok(self.db.accounts().filter_options(&ids, executor).await?)
    }

    /// 同一授权边界内的采购负责人候选；销售关联查询返回空候选。
    async fn owner_options_purchase(
        &self,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FilterOption>> {
        let Some(scope) = authorization.purchase_scope.as_ref() else {
            return Ok(Vec::new());
        };
        if authorization.empty() {
            return Ok(Vec::new());
        }
        let ids = self.db.purchase_orders().current_owner_ids(scope, executor).await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("负责人候选超过查询上限".into()));
        }
        Ok(self.db.accounts().filter_options(&ids, executor).await?)
    }
}

/// 单据行关联的多个责任事实中任一通过公共判定即该行可见。
fn row_visible(
    access: &FundsResolvedScope,
    orders: &[(LinkedOrderId, Option<String>, Option<String>, String, u64)],
    operators: &[String],
    secondary: &[String],
) -> Result<bool> {
    if orders.is_empty() {
        return FundsAccess::allows(
            access,
            &FundsLinkedFacts {
                owner_user_id: None,
                business_org_unit_id: None,
                operator_user_ids: operators.to_vec(),
                secondary_operator_user_ids: secondary.to_vec(),
                linked_document_id: String::new(),
                linked_document_version: 0,
            },
        );
    }
    for (_, owner, org, id, version) in orders {
        let facts = FundsLinkedFacts {
            owner_user_id: owner.clone(),
            business_org_unit_id: org.clone(),
            operator_user_ids: operators.to_vec(),
            secondary_operator_user_ids: secondary.to_vec(),
            linked_document_id: id.clone(),
            linked_document_version: *version,
        };
        if FundsAccess::allows(access, &facts)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 多关联单据行的条件匹配：负责人与组织任一关联命中，操作人按单据事实判定。
fn matches_multi_condition(
    orders: &[(LinkedOrderId, Option<String>, Option<String>, String, u64)],
    operators: &[String],
    secondary: &[String],
    condition: &FundsLinkedCondition,
) -> bool {
    if let Some(wanted) = &condition.operator_user_ids
        && !operators.iter().any(|id| wanted.iter().any(|item| item == id))
    {
        return false;
    }
    if let Some(wanted) = &condition.secondary_operator_user_ids
        && !secondary.iter().any(|id| wanted.iter().any(|item| item == id))
    {
        return false;
    }
    if condition.owner_user_ids.is_none() && condition.org_unit_ids.is_none() {
        return true;
    }
    orders.iter().any(|(linked, owner, org, _, _)| {
        linked.is_some()
            && condition
                .owner_user_ids
                .as_ref()
                .is_none_or(|wanted| owner.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
            && condition
                .org_unit_ids
                .as_ref()
                .is_none_or(|wanted| org.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
    })
}

// __funds_scope_chunk_b__

use erp_finance::entity::payable::AllocationAction as PayableAllocationAction;
use erp_finance::entity::receivable::AllocationAction as ReceivableAllocationAction;

impl FundsAccess {
    /// 回款分配按核销分录反查所属子账与销售单；金额按正反动作记方向。
    async fn receipt_matched_links(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<ReceiptLink>>> {
        use erp_core::ids::{CustomerReceiptId, ReceivableEntryId};
        let keys = receipt_ids.iter().map(|id| CustomerReceiptId::new(id.clone())).collect::<Vec<_>>();
        let allocations = self.db.receipt_allocations().find_allocations_by_receipts(&keys, executor).await?;
        let entry_keys = allocations
            .iter()
            .map(|item| ReceivableEntryId::new(item.receivable_entry_id.to_string()))
            .collect::<Vec<_>>();
        let entries = self.db.receivable_entries().find_entries_by_ids(&entry_keys, executor).await?;
        let entry_account = entries
            .into_iter()
            .map(|entry| (entry.base.id.clone(), entry.receivable_account_id.to_string()))
            .collect::<HashMap<_, _>>();
        let account_keys = entry_account.values().cloned().collect::<Vec<_>>();
        let accounts = self.db.receivable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account.sales_order_id.to_string()))
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<ReceiptLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                ReceivableAllocationAction::Apply => item.allocated_amount,
                ReceivableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_amount),
            };
            let order = entry_account
                .get(&item.receivable_entry_id.to_string())
                .and_then(|account| account_order.get(account).cloned());
            let view = receipt_allocation_view(&item);
            let id = item.base.id.clone();
            let receipt = item.customer_receipt_id.to_string();
            links.entry(receipt).or_default().push(ReceiptLink { id, signed, order, view });
        }
        Ok(links)
    }

    /// 销项发票分配按子账反查销售单；含税金额按正反动作记方向。
    async fn sales_invoice_matched_links(
        &self,
        invoice_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<SalesInvoiceLink>>> {
        use erp_core::ids::InvoiceId;
        let keys = invoice_ids.iter().map(|id| InvoiceId::new(id.clone())).collect::<Vec<_>>();
        let allocations =
            self.db.sales_invoice_allocations().find_allocations_by_invoices(&keys, executor).await?;
        let account_keys =
            allocations.iter().map(|item| item.receivable_account_id.to_string()).collect::<Vec<_>>();
        let accounts = self.db.receivable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account.sales_order_id.to_string()))
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<SalesInvoiceLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                ReceivableAllocationAction::Apply => item.allocated_gross_amount,
                ReceivableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
            };
            let order = account_order.get(&item.receivable_account_id.to_string()).cloned();
            let view = sales_invoice_allocation_view(&item);
            let id = item.base.id.clone();
            let invoice = item.invoice_id.to_string();
            links.entry(invoice).or_default().push(SalesInvoiceLink { id, signed, order, view });
        }
        Ok(links)
    }

    /// 付款核销按应付分录反查子账与采购单；结算单来源份额无采购归属。
    async fn payment_matched_links(
        &self,
        payment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<PaymentLink>>> {
        use erp_core::ids::{PayableAccountId, PayableEntryId, SupplierPaymentId};
        use erp_finance::entity::payable::PayableSourceType;
        let keys = payment_ids.iter().map(|id| SupplierPaymentId::new(id.clone())).collect::<Vec<_>>();
        let allocations = self.db.payment_allocations().find_allocations_by_payments(&keys, executor).await?;
        let entry_keys = allocations
            .iter()
            .map(|item| PayableEntryId::new(item.payable_entry_id.to_string()))
            .collect::<Vec<_>>();
        let entries = self.db.payable_entries().find_entries_by_ids(&entry_keys, executor).await?;
        let entry_account = entries
            .into_iter()
            .map(|entry| (entry.base.id.clone(), entry.payable_account_id.to_string()))
            .collect::<HashMap<_, _>>();
        let account_keys =
            entry_account.values().map(|id| PayableAccountId::new(id.clone())).collect::<Vec<_>>();
        let accounts = self.db.payable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| {
                let order = (account.source_type == PayableSourceType::PurchaseOrder)
                    .then(|| account.source_document_id.clone());
                (account.base.id.clone(), order)
            })
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<PaymentLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                PayableAllocationAction::Apply => item.allocated_amount,
                PayableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_amount),
            };
            let order = entry_account
                .get(&item.payable_entry_id.to_string())
                .and_then(|account| account_order.get(account).cloned())
                .flatten();
            let view = erp_finance::dto::payable::PaymentAllocationView::from(&item);
            let id = item.base.id.clone();
            let payment = item.supplier_payment_id.to_string();
            links.entry(payment).or_default().push(PaymentLink { id, signed, order, view });
        }
        Ok(links)
    }

    /// 审计事实按资源批量取回经办人；动作前缀不匹配的审计不计入。
    async fn audit_operators(
        &self,
        resource: &str,
        ids: &[String],
        keep: impl Fn(&str) -> bool,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<String>>> {
        let pairs = ids.iter().map(|id| (resource.to_string(), id.clone())).collect::<Vec<_>>();
        let facts = self.db.audit_logs().list_separation_facts_by_resources(&pairs, executor).await?;
        let mut operators: HashMap<String, Vec<String>> = HashMap::new();
        for fact in facts {
            if fact.resource_type == resource && keep(&fact.action) {
                if let Some(id) = fact.resource_id {
                    operators.entry(id).or_default().push(fact.actor_id);
                }
            }
        }
        for list in operators.values_mut() {
            list.sort();
            list.dedup();
        }
        Ok(operators)
    }

    /// 回款核销经办人取审批提交快照的提交人；未提交的草稿无核销经办人。
    async fn receipt_settle_operators(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<String>>> {
        use erp_workflow::entity::document_registry::DocumentType;
        let objects =
            receipt_ids.iter().map(|id| (DocumentType::CustomerReceipt, id.clone())).collect::<Vec<_>>();
        let snapshots =
            self.db.approval_subject_snapshots().list_by_business_objects(&objects, executor).await?;
        let mut operators: HashMap<String, Vec<String>> = HashMap::new();
        for snapshot in snapshots {
            operators
                .entry(snapshot.business_object_id.clone())
                .or_default()
                .push(snapshot.payload.submitted_by.clone());
        }
        for list in operators.values_mut() {
            list.sort();
            list.dedup();
        }
        Ok(operators)
    }

    /// 工作项当前处理人；已关闭任务回退完成人，缺失任务按无处理人。
    async fn work_item_handlers(
        &self,
        work_item_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let mut handlers = HashMap::new();
        let mut unique = work_item_ids.to_vec();
        unique.sort();
        unique.dedup();
        for id in unique {
            if id.is_empty() {
                continue;
            }
            if let Some(item) = self.db.work_items().find_work_item(&id, executor).await? {
                if let Some(handler) = item.owner_user_id.clone().or(item.completed_by.clone()) {
                    handlers.insert(id.clone(), handler);
                }
            }
        }
        Ok(handlers)
    }
}

/// 分配金额求和的零值；与应收映射保持同一零金额口径。
fn zero_amount() -> Amount {
    erp_finance::service::receivable::mapping::zero_amount()
}

// __funds_scope_chunk_c__

/// 同一快照的匹配份额汇总；未分配单列，整单合计部分授权为 null。
fn build_summary(
    matched: &[(String, Amount, LinkedOrderId)],
    owner_of: &HashMap<String, String>,
    whole_total: Option<Amount>,
    scope_version: &str,
    permission_limited: bool,
) -> Result<FundsSummaryView> {
    let (grouped, unassigned) = summarize_matched_shares(matched, |order| owner_of.get(order).cloned())?;
    let mut grouped = grouped
        .into_iter()
        .map(|(owner_user_id, visible_share)| FundsPersonShare { owner_user_id, visible_share })
        .collect::<Vec<_>>();
    grouped.sort_by(|left, right| left.owner_user_id.cmp(&right.owner_user_id));
    Ok(FundsSummaryView {
        grouped,
        unassigned,
        whole_total,
        permission_limited,
        scope_version: scope_version.to_string(),
    })
}

/// M07 回款与 M09 付款共用的单据可见性：整单、有匹配份额或仅有未分配份额时可见。
fn keep_row(visible: bool, whole: bool, matched_any: bool, all_unlinked_or_empty: bool) -> bool {
    visible && (whole || matched_any || all_unlinked_or_empty)
}

/// 回款分配实体转响应视图；与现有回款投影保持同一字段口径。
fn receipt_allocation_view(
    item: &erp_finance::entity::receivable::ReceiptAllocation,
) -> erp_finance::dto::receivable::ReceiptAllocationView {
    erp_finance::dto::receivable::ReceiptAllocationView {
        id: item.base.id.clone(),
        allocation_seq: item.allocation_seq,
        allocation_action: item.allocation_action,
        receivable_entry_id: item.receivable_entry_id.to_string(),
        allocated_amount: item.allocated_amount,
        allocated_at: item.allocated_at,
        reverses_allocation_id: item.reverses_allocation_id.as_ref().map(|id| id.to_string()),
    }
}

/// 销项发票分配实体转响应视图；与发票查询服务保持同一字段口径。
fn sales_invoice_allocation_view(
    item: &erp_finance::entity::receivable::SalesInvoiceAllocation,
) -> erp_finance::dto::receivable::SalesInvoiceAllocationView {
    erp_finance::dto::receivable::SalesInvoiceAllocationView {
        id: item.base.id.clone(),
        allocation_seq: item.allocation_seq,
        allocation_action: item.allocation_action,
        receivable_account_id: item.receivable_account_id.to_string(),
        allocated_gross_amount: item.allocated_gross_amount,
        allocated_net_amount: item.allocated_net_amount,
        allocated_tax_amount: item.allocated_tax_amount,
        reverses_allocation_id: item.reverses_allocation_id.as_ref().map(|id| id.to_string()),
    }
}

// __funds_scope_chunk_d2__

/// 单据行关联责任元组：关联单据、负责人、组织、事实主键与版本。
type OrderTuple = (LinkedOrderId, Option<String>, Option<String>, String, u64);

/// 回款行关联元组；缺失订单的分配保留未分配归属，不丢份额。
fn receipt_tuples(
    row: &CustomerReceiptRow,
    links: &[ReceiptLink],
    facts: &HashMap<String, LinkedSalesFact>,
) -> Vec<OrderTuple> {
    let mut seen = BTreeSet::new();
    let mut tuples = Vec::new();
    for link in links {
        let key = link.order.clone().unwrap_or_default();
        if !seen.insert(key) {
            continue;
        }
        match link.order.as_ref().and_then(|id| facts.get(id)) {
            Some(fact) => tuples.push((
                link.order.clone(),
                Some(fact.owner_user_id.clone()),
                Some(fact.business_org_unit_id.clone()),
                row.id.clone(),
                row.version,
            )),
            None => tuples.push((None, None, None, row.id.clone(), row.version)),
        }
    }
    tuples
}

/// 授权集合内的归属订单；`None` 表示公司范围不限制关联单。
fn matched_orders(tuples: &[OrderTuple], allowed: &Option<BTreeSet<String>>) -> Vec<String> {
    tuples
        .iter()
        .filter_map(|(order, _, _, _, _)| order.clone())
        .filter(|order| allowed.as_ref().is_none_or(|set| set.contains(order)))
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// 整单口径求和；仅整单读取资格持有者可见的结果使用，不得用于部分授权。
fn sum_all(links: &[ReceiptLink]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        total = total.checked_add(link.signed);
    }
    total
}

/// 汇总输入：匹配份额与未分配份额进入汇总，未授权订单份额不得进入。
fn summary_inputs(links: &[ReceiptLink], matched: &[String]) -> Vec<(String, Amount, LinkedOrderId)> {
    links
        .iter()
        .filter(|link| link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)))
        .map(|link| (link.id.clone(), link.signed, link.order.clone()))
        .collect()
}

/// 匹配份额求和；方向已在装载时按正反动作记入金额符号，不做差额推导。
fn sum_signed(links: &[ReceiptLink], matched: &[String]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        if !link.order.as_ref().is_some_and(|order| matched.iter().any(|id| id == order)) {
            continue;
        }
        total = total.checked_add(link.signed);
    }
    total
}

impl FundsAccess {
    /// 分页查询客户回款范围行：负责销售与登记/核销经办人分别查询。
    pub async fn customer_receipt_list_scoped(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot =
            self.checked_customer_receipts(params, &query, actor, params.scope_version.as_deref()).await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析回款详情动作；不可见与不存在统一为 NotFound。
    pub async fn customer_receipt_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<FundsScopedResult<ScopedCustomerReceiptRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_customer_receipt_detail(&id, &actor, executor).await })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    async fn checked_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        let snapshot = self.snapshot_customer_receipts(params, query, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_customer_receipts(params, query, actor).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_customer_receipts(&params, &query, &actor, executor).await })
            })
            .await
    }

    /// 业务筛选复用现有仓储查询，授权过滤与金额裁剪在同一事务内完成。
    async fn load_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        use erp_finance::dto::receivable::SortDir;
        let (access, authorization) = self.resolve(actor, "customer_receipt", "list", executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "回款无可见范围"));
        }
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Receipt,
        )
        .await?;
        let scope_query = erp_finance::repository::ScopedCustomerReceiptQuery {
            keyword_ids,
            receipt_no: query.receipt_no.clone(),
            counterparty_party_id: query.counterparty_party_id.clone(),
            status: query.status,
            scope: erp_finance::repository::ReceivableListScope {
                sales_order_id: query.sales_order_id.clone(),
                receivable_account_id: query.receivable_account_id.clone(),
            },
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let candidates =
            self.db.receivable().search_customer_receipts_in_account_scope(&scope_query, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("回款查询超过上限，请收窄组织或负责人条件".into()));
        }
        let decided = self
            .assemble_customer_receipts(query, candidates.items, access, &authorization, executor)
            .await?;
        let ids = decided.iter().map(|(row, _)| row.id.clone()).collect::<Vec<_>>();
        let links = self.receipt_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, matched) in decided.iter() {
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            for order in matched.iter().filter_map(|id| facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_customer_receipts(
            params,
            query,
            decided,
            links,
            facts,
            authorization,
            fingerprint,
            executor,
        )
        .await
    }
}

// __funds_scope_chunk_d__

use erp_finance::repository::CustomerReceiptRow;

impl FundsAccess {
    /// 回款经办人事实：登记取创建审计，核销取审批提交人；无过滤条件时不读审计。
    async fn receipt_operators(
        &self,
        ids: &[String],
        kind: Option<erp_finance::dto::receivable::ReceiptOperatorKind>,
        want: bool,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<String>>> {
        use erp_finance::dto::receivable::ReceiptOperatorKind;
        if !want {
            return Ok(HashMap::new());
        }
        match kind {
            Some(ReceiptOperatorKind::Register) => {
                self.audit_operators(
                    "customer_receipt",
                    ids,
                    |action| action == "customer_receipt.create",
                    executor,
                )
                .await
            },
            Some(ReceiptOperatorKind::Settle) => self.receipt_settle_operators(ids, executor).await,
            None => Ok(HashMap::new()),
        }
    }

    /// 回款关联筛选条件：负责人、经办人与组织分别精确匹配，同字段 OR、异字段 AND。
    async fn receipt_condition(
        &self,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().iter().cloned().collect::<Vec<_>>();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query
                .sales_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            operator_user_ids: query
                .operator_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }

    /// 回款候选逐行判定可见性与筛选；授权集合外的关联份额不进入行与汇总。
    async fn assemble_customer_receipts(
        &self,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        rows: Vec<CustomerReceiptRow>,
        access: FundsResolvedScope,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(CustomerReceiptRow, Vec<String>)>> {
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let links = self.receipt_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let authorized = self.authorized_sales_ids(authorization, executor).await?;
        let allowed = authorized.map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let operators = self
            .receipt_operators(&ids, query.operator_kind, query.operator_user_ids.is_some(), executor)
            .await?;
        let condition = self.receipt_condition(query, executor).await?;
        let whole = authorization.whole();
        let mut decided = Vec::new();
        for row in rows {
            let empty_links: Vec<ReceiptLink> = Vec::new();
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            let tuples = receipt_tuples(&row, row_links, &facts);
            let matched = matched_orders(&tuples, &allowed);
            let visible = row_visible(&access, &tuples, &[], &[])?;
            let unlinked = row_links.iter().all(|link| link.order.is_none());
            if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
                continue;
            }
            let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
            if !matches_multi_condition(&tuples, &doc_operators, &[], &condition) {
                continue;
            }
            decided.push((row, matched));
        }
        Ok(decided)
    }
}

impl FundsAccess {
    /// 回款候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    async fn finish_customer_receipts(
        &self,
        params: &erp_finance::dto::receivable::CustomerReceiptListParams,
        query: &erp_finance::dto::receivable::CustomerReceiptListQuery,
        decided: Vec<(CustomerReceiptRow, Vec<String>)>,
        links: HashMap<String, Vec<ReceiptLink>>,
        facts: HashMap<String, LinkedSalesFact>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedCustomerReceiptRow>> {
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = query.paging.page_size.max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let whole = authorization.whole();
        let empty_links: Vec<ReceiptLink> = Vec::new();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, matched) in decided[start..end].iter() {
                let row_links = links.get(&row.id).unwrap_or(&empty_links);
                items.push(self.cut_receipt_row(row, row_links, matched, whole));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        for (row, matched) in decided.iter() {
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            triples.extend(summary_inputs(row_links, matched));
            whole_sum = whole_sum.checked_add(row.amount);
        }
        let owner_of = facts
            .iter()
            .map(|(id, fact)| (id.clone(), fact.owner_user_id.clone()))
            .collect::<HashMap<_, _>>();
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = build_summary(&triples, &owner_of, whole_amount(whole, whole_sum), &version, !whole)?;
        let owner_options = self.owner_options_sales(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "回款按核销关联销售当前负责人与登记/核销经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_owner_and_receipt_operator",
        })
    }

    /// 单行金额裁剪；整单金额与完整分配仅整单资格返回，否则为 null。
    fn cut_receipt_row(
        &self,
        row: &CustomerReceiptRow,
        links: &[ReceiptLink],
        matched: &[String],
        whole: bool,
    ) -> ScopedCustomerReceiptRow {
        let mut views: Vec<_> = if whole {
            links.iter().map(|link| link.view.clone()).collect()
        } else {
            links
                .iter()
                .filter(|link| link.order.as_ref().is_some_and(|order| matched.iter().any(|id| id == order)))
                .map(|link| link.view.clone())
                .collect()
        };
        views.sort_by_key(|view| view.allocation_seq);
        let net = sum_all(links);
        ScopedCustomerReceiptRow {
            id: row.id.clone(),
            receipt_no: row.receipt_no.clone(),
            status: row.status,
            received_at: row.received_at,
            created_at: row.created_at,
            visible_allocated_share: sum_signed(links, matched),
            amount: whole_amount(whole, row.amount),
            allocated_total: whole_amount(whole, net),
            unallocated_amount: whole_amount(whole, row.amount.checked_sub(net)),
            allocations: Some(views),
            permission_limited: !whole,
        }
    }

    /// 回款详情同一事务内解析、取数与裁剪；版本绑定关联单据。
    async fn load_customer_receipt_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedCustomerReceiptRow>> {
        use erp_finance::repository::CustomerReceiptFilter;
        let (access, authorization) = self.resolve(actor, "customer_receipt", "detail", executor).await?;
        let filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: Some(vec![id.to_string()]),
            pending_entry_ids: Vec::new(),
            receipt_no: None,
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.customer_receipts().search_customer_receipts(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
        let links = self.receipt_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let authorized = self.authorized_sales_ids(&authorization, executor).await?;
        let allowed = authorized.map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<ReceiptLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = receipt_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = authorization.whole();
        let visible = row_visible(&access, &tuples, &[], &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("客户回款单不存在".into()));
        }
        let data = self.cut_receipt_row(&row, row_links, &matched, whole);
        let mut parts = vec![format!("{}:{}", row.id, row.version)];
        for order in matched.iter().filter_map(|id| facts.get(id)) {
            parts.push(format!("{}:{}", order.owner_user_id, order.version));
        }
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "回款按核销关联销售当前负责人与登记/核销经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_owner_and_receipt_operator",
        })
    }
}

// __funds_scope_chunk_e__

/// 进项发票分配的授权裁剪单元。
struct PurchaseInvoiceLink {
    /// 分配主键。
    id: String,
    /// 正反动作后的含税方向金额。
    signed: Amount,
    /// 归属采购单；结算单来源与缺失时计入未分配。
    order: LinkedOrderId,
    /// 响应视图。
    view: erp_finance::dto::payable::PurchaseInvoiceAllocationView,
}

/// 采购关联整单读取资格：资金与关联采购均具未被个人上限收窄的公司范围。
fn purchase_whole(authorization: &FundsAuthorization) -> bool {
    authorization.purchase_scope.as_ref().is_some_and(|scope| scope.is_company())
        && authorization.funds.is_company()
}

impl FundsAccess {
    /// 销售与采购双关联同时证明；发票双方向查询使用，单方向复用 resolve 即可。
    pub async fn resolve_dual(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<(FundsResolvedScope, FundsAuthorization)> {
        let access = self.scope.resolve(actor, resource, action, executor).await.map_err(Error::from)?;
        let (sales_access, sales) =
            self.linked_sales_scope(actor, &access, executor).await.map_err(Error::from)?;
        let (purchase_resolved, purchase_scope) =
            self.linked_purchase_scope(actor, purchase_access, executor).await.map_err(Error::from)?;
        let mut authorization = FundsAuthorization {
            sales,
            funds: sales_scope(&access_output(&access)?, actor.id(), &[], Vec::new()),
            purchase_scope: Some(purchase_scope),
            context: access_output(&access)?,
            fingerprint: Default::default(),
            no_scope: false,
        };
        sales_access.scope_version.hash(&mut authorization.fingerprint);
        purchase_resolved.scope_version.hash(&mut authorization.fingerprint);
        authorization.context.scope_version.hash(&mut authorization.fingerprint);
        authorization.no_scope = authorization.empty();
        Ok((access, authorization))
    }

    /// 进项发票分配按应付子账反查采购单；结算单来源份额无采购归属。
    async fn purchase_invoice_matched_links(
        &self,
        invoice_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<PurchaseInvoiceLink>>> {
        use erp_core::ids::{InvoiceId, PayableAccountId};
        use erp_finance::entity::payable::PayableSourceType;
        let keys = invoice_ids.iter().map(|id| InvoiceId::new(id.clone())).collect::<Vec<_>>();
        let allocations =
            self.db.purchase_invoice_allocations().find_allocations_by_invoices(&keys, executor).await?;
        let account_keys = allocations
            .iter()
            .map(|item| PayableAccountId::new(item.payable_account_id.to_string()))
            .collect::<Vec<_>>();
        let accounts = self.db.payable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| {
                let order = (account.source_type == PayableSourceType::PurchaseOrder)
                    .then(|| account.source_document_id.clone());
                (account.base.id.clone(), order)
            })
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<PurchaseInvoiceLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                PayableAllocationAction::Apply => item.allocated_gross_amount,
                PayableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
            };
            let order = account_order.get(&item.payable_account_id.to_string()).cloned().flatten();
            let view = erp_finance::dto::payable::PurchaseInvoiceAllocationView {
                id: item.base.id.clone(),
                invoice_id: item.invoice_id.to_string(),
                allocation_seq: item.allocation_seq,
                allocation_action: item.allocation_action,
                payable_account_id: item.payable_account_id.to_string(),
                allocated_gross_amount: item.allocated_gross_amount,
                allocated_net_amount: item.allocated_net_amount,
                allocated_tax_amount: item.allocated_tax_amount,
                reverses_allocation_id: item.reverses_allocation_id.as_ref().map(|id| id.to_string()),
            };
            let id = item.base.id.clone();
            let invoice = item.invoice_id.to_string();
            links.entry(invoice).or_default().push(PurchaseInvoiceLink { id, signed, order, view });
        }
        Ok(links)
    }
}

// __funds_scope_chunk_f__

use erp_finance::repository::InvoiceRow;

impl FundsAccess {
    /// 分页查询发票范围行：销项按负责销售，登记经办人与组织分别查询。
    pub async fn invoice_list_scoped(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_invoices(params, &query, actor, purchase_access, params.scope_version.as_deref())
            .await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析发票详情动作；不可见与不存在统一为 NotFound。
    pub async fn invoice_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedResult<ScopedInvoiceRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { this.load_invoice_detail(&id, &actor, &purchase_access, executor).await },
                )
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    async fn checked_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        let snapshot = self.snapshot_invoices(params, query, actor, purchase_access).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_invoices(params, query, actor, purchase_access).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_invoices(&params, &query, &actor, &purchase_access, executor).await
                })
            })
            .await
    }
}

// __funds_scope_chunk_g__

impl FundsAccess {
    /// 分页查询发票范围行：销项按负责销售与登记经办，进项按采购负责人与登记经办。
    async fn load_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        use erp_finance::dto::receivable::SortDir;
        use erp_finance::entity::receivable::InvoiceDirection;
        let (access, authorization) =
            self.resolve_dual(actor, "invoice", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "发票无可见范围"));
        }
        let target = match query.invoice_direction {
            Some(InvoiceDirection::Purchase) => {
                erp_finance::repository::keyword::FinanceSearchTarget::PurchaseInvoice
            },
            _ => erp_finance::repository::keyword::FinanceSearchTarget::SalesInvoice,
        };
        let keyword_ids = crate::finance::search::keyword_ids(&self.db, query.q.as_deref(), target).await?;
        let filter = erp_finance::repository::InvoiceFilter {
            keyword_ids,
            invoice_ids: None,
            invoice_direction: query.invoice_direction,
            invoice_kind: query.invoice_kind,
            party_id: query.party_id.clone(),
            invoice_no: query.invoice_no.clone(),
            status: query.status,
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let candidates = self.db.invoices().search_invoices(&filter, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("发票查询超过上限，请收窄组织或负责人条件".into()));
        }
        let decided = self
            .assemble_invoices(query, candidates.items, &access, &authorization, purchase_access, executor)
            .await?;
        let ids: Vec<String> = decided.iter().map(|(row, _, _)| row.id.clone()).collect();
        let sales_links = self.sales_invoice_matched_links(&ids, executor).await?;
        let purchase_links = self.purchase_invoice_matched_links(&ids, executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, sales_matched, purchase_matched) in decided.iter() {
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            for order in sales_matched.iter().filter_map(|id| sales_facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
            for order in purchase_matched.iter().filter_map(|id| purchase_facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_invoices(
            params,
            query,
            decided,
            sales_links,
            purchase_links,
            sales_facts,
            purchase_facts,
            authorization,
            fingerprint,
            executor,
        )
        .await
    }

    /// 发票双方向关联事实一次取回；缺失订单保留未分配归属。
    async fn invoice_fact_maps(
        &self,
        sales_links: &HashMap<String, Vec<SalesInvoiceLink>>,
        purchase_links: &HashMap<String, Vec<PurchaseInvoiceLink>>,
        executor: &mut dyn Executor,
    ) -> Result<(HashMap<String, LinkedSalesFact>, HashMap<String, LinkedPurchaseFact>)> {
        let sales_ids =
            sales_links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let purchase_ids =
            purchase_links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let sales = self.sales_fact_map(&sales_ids, executor).await?;
        let purchase = self.purchase_fact_map(&purchase_ids, executor).await?;
        Ok((sales, purchase))
    }

    /// 发票候选逐行判定可见性与筛选；授权集合外的关联份额不进入行与汇总。
    #[allow(clippy::too_many_arguments)]
    async fn assemble_invoices(
        &self,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        rows: Vec<InvoiceRow>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(InvoiceRow, Vec<String>, Vec<String>)>> {
        use erp_finance::entity::receivable::InvoiceDirection;
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let sales_links = self.sales_invoice_matched_links(&ids, executor).await?;
        let purchase_links = self.purchase_invoice_matched_links(&ids, executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let sales_allowed = self
            .authorized_sales_ids(authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let purchase_allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let condition = self.invoice_condition(query, executor).await?;
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let mut decided = Vec::new();
        for row in rows {
            let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
            let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
            let sales_tuples = invoice_sales_tuples(&row, sales, &sales_facts);
            let purchase_tuples = invoice_purchase_tuples(&row, purchase, &purchase_facts);
            let sales_matched = matched_orders(&sales_tuples, &sales_allowed);
            let purchase_matched = matched_orders(&purchase_tuples, &purchase_allowed);
            let whole = match row.invoice_direction {
                InvoiceDirection::Sales => authorization.whole(),
                InvoiceDirection::Purchase => purchase_whole(authorization),
            };
            let mut combined = sales_tuples.clone();
            combined.extend(purchase_tuples.clone());
            let operators = vec![row.stable.created_by.clone()];
            let visible = row_visible(access, &combined, &operators, &[])?;
            let unlinked = combined.iter().all(|(order, _, _, _, _)| order.is_none());
            let matched_any = !sales_matched.is_empty() || !purchase_matched.is_empty();
            if !keep_row(visible, whole, matched_any, sales.is_empty() && purchase.is_empty() || unlinked) {
                continue;
            }
            if !matches_invoice_condition(&sales_tuples, &purchase_tuples, &operators, &condition) {
                continue;
            }
            if !matches_invoice_business(query, &row, sales, &sales_matched) {
                continue;
            }
            decided.push((row, sales_matched, purchase_matched));
        }
        Ok(decided)
    }

    /// 发票关联筛选条件：销售负责人、采购负责人、登记经办人与组织分别精确匹配。
    async fn invoice_condition(
        &self,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().iter().cloned().collect::<Vec<_>>();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query
                .sales_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            operator_user_ids: query
                .operator_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            secondary_operator_user_ids: query
                .procurement_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            org_unit_ids,
        })
    }

    /// 发票候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    #[allow(clippy::too_many_arguments)]
    async fn finish_invoices(
        &self,
        params: &erp_finance::dto::receivable::InvoiceListParams,
        query: &erp_finance::dto::receivable::InvoiceListQuery,
        decided: Vec<(InvoiceRow, Vec<String>, Vec<String>)>,
        sales_links: HashMap<String, Vec<SalesInvoiceLink>>,
        purchase_links: HashMap<String, Vec<PurchaseInvoiceLink>>,
        sales_facts: HashMap<String, LinkedSalesFact>,
        purchase_facts: HashMap<String, LinkedPurchaseFact>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRow>> {
        use erp_finance::entity::receivable::InvoiceDirection;
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = query.paging.page_size.max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, sales_matched, purchase_matched) in decided[start..end].iter() {
                let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
                let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
                let whole = match row.invoice_direction {
                    InvoiceDirection::Sales => authorization.whole(),
                    InvoiceDirection::Purchase => purchase_whole(&authorization),
                };
                items.push(self.cut_invoice_row(
                    row,
                    sales,
                    purchase,
                    sales_matched,
                    purchase_matched,
                    whole,
                ));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        let mut all_whole = true;
        for (row, sales_matched, purchase_matched) in decided.iter() {
            let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
            let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
            let whole = match row.invoice_direction {
                InvoiceDirection::Sales => authorization.whole(),
                InvoiceDirection::Purchase => purchase_whole(&authorization),
            };
            all_whole &= whole;
            triples.extend(invoice_summary_inputs(sales, purchase, sales_matched, purchase_matched));
            if whole {
                whole_sum = whole_sum.checked_add(row.gross_amount);
            }
        }
        let mut owner_of = HashMap::new();
        for (id, fact) in sales_facts.iter() {
            owner_of.insert(id.clone(), fact.owner_user_id.clone());
        }
        for (id, fact) in purchase_facts.iter() {
            if let Some(owner) = fact.owner_user_id.clone() {
                owner_of.insert(id.clone(), owner);
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary =
            build_summary(&triples, &owner_of, whole_amount(all_whole, whole_sum), &version, !all_whole)?;
        let owner_options = self.owner_options_merged(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "发票按分配关联销售/采购当前负责人与登记经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_and_purchase_owner_and_register_operator",
        })
    }

    /// 销售与采购双边界的负责人候选合并；候选不授予命令资格。
    async fn owner_options_merged(
        &self,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FilterOption>> {
        let mut options = self.owner_options_sales(authorization, executor).await?;
        options.extend(self.owner_options_purchase(authorization, executor).await?);
        options.sort_by(|left, right| left.value.cmp(&right.value).then(left.label.cmp(&right.label)));
        options.dedup_by(|next, prev| next.value == prev.value);
        Ok(options)
    }

    /// 单行金额裁剪；整单金额与完整分配仅整单资格返回，否则为 null。
    fn cut_invoice_row(
        &self,
        row: &InvoiceRow,
        sales: &[SalesInvoiceLink],
        purchase: &[PurchaseInvoiceLink],
        sales_matched: &[String],
        purchase_matched: &[String],
        whole: bool,
    ) -> ScopedInvoiceRow {
        let mut allocations: Vec<_> = if whole {
            sales.iter().map(|link| link.view.clone()).collect()
        } else {
            sales
                .iter()
                .filter(|link| {
                    link.order.as_ref().is_some_and(|order| sales_matched.iter().any(|id| id == order))
                })
                .map(|link| link.view.clone())
                .collect()
        };
        allocations.sort_by_key(|view| view.allocation_seq);
        let mut purchase_views: Vec<_> = if whole {
            purchase.iter().map(|link| link.view.clone()).collect()
        } else {
            purchase
                .iter()
                .filter(|link| {
                    link.order.as_ref().is_some_and(|order| purchase_matched.iter().any(|id| id == order))
                })
                .map(|link| link.view.clone())
                .collect()
        };
        purchase_views.sort_by_key(|view| view.allocation_seq);
        let allocated = sum_invoice_signed(sales, purchase);
        ScopedInvoiceRow {
            id: row.id.clone(),
            invoice_no: row.invoice_no.clone(),
            invoice_direction: row.invoice_direction,
            invoice_kind: row.invoice_kind,
            status: row.stable.status(),
            invoice_date: row.invoice_date,
            created_at: row.created_at,
            visible_allocated_share: sum_invoice_matched(sales, purchase, sales_matched, purchase_matched),
            gross_amount: whole_amount(whole, row.gross_amount),
            allocated_total: whole_amount(whole, allocated),
            unallocated_amount: whole_amount(whole, invoice_unallocated(row, allocated)),
            allocations: Some(allocations),
            purchase_allocations: Some(purchase_views),
            permission_limited: !whole,
        }
    }

    /// 发票详情同一事务内解析、取数与裁剪；版本绑定关联单据。
    async fn load_invoice_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRow>> {
        use erp_finance::entity::receivable::InvoiceDirection;
        use erp_finance::repository::InvoiceFilter;
        let (access, authorization) =
            self.resolve_dual(actor, "invoice", "detail", purchase_access, executor).await?;
        let filter = InvoiceFilter {
            keyword_ids: None,
            invoice_ids: Some(vec![id.to_string()]),
            invoice_direction: None,
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.invoices().search_invoices(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("发票不存在".into()))?;
        let sales_links = self.sales_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let purchase_links =
            self.purchase_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
        let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
        let sales_tuples = invoice_sales_tuples(&row, sales, &sales_facts);
        let purchase_tuples = invoice_purchase_tuples(&row, purchase, &purchase_facts);
        let sales_allowed = self
            .authorized_sales_ids(&authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let purchase_allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let sales_matched = matched_orders(&sales_tuples, &sales_allowed);
        let purchase_matched = matched_orders(&purchase_tuples, &purchase_allowed);
        let whole = match row.invoice_direction {
            InvoiceDirection::Sales => authorization.whole(),
            InvoiceDirection::Purchase => purchase_whole(&authorization),
        };
        let mut combined = sales_tuples;
        combined.extend(purchase_tuples);
        let operators = vec![row.stable.created_by.clone()];
        let visible = row_visible(&access, &combined, &operators, &[])?;
        let unlinked = combined.iter().all(|(order, _, _, _, _)| order.is_none());
        let matched_any = !sales_matched.is_empty() || !purchase_matched.is_empty();
        if !keep_row(visible, whole, matched_any, sales.is_empty() && purchase.is_empty() || unlinked) {
            return Err(Error::NotFound("发票不存在".into()));
        }
        let data = self.cut_invoice_row(&row, sales, purchase, &sales_matched, &purchase_matched, whole);
        let mut parts = vec![format!("{}:{}", row.id, row.version)];
        for order in sales_matched.iter().filter_map(|id| sales_facts.get(id)) {
            parts.push(format!("{}:{}", order.owner_user_id, order.version));
        }
        for order in purchase_matched.iter().filter_map(|id| purchase_facts.get(id)) {
            parts.push(format!("{}:{:?}:{}", id, order.owner_user_id, order.version));
        }
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "发票按分配关联销售/采购当前负责人与登记经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_sales_and_purchase_owner_and_register_operator",
        })
    }
}

/// 发票销项分配关联元组；缺失订单的分配保留未分配归属，不丢份额。
fn invoice_sales_tuples(
    row: &InvoiceRow,
    links: &[SalesInvoiceLink],
    facts: &HashMap<String, LinkedSalesFact>,
) -> Vec<OrderTuple> {
    let mut seen = BTreeSet::new();
    let mut tuples = Vec::new();
    for link in links {
        let key = link.order.clone().unwrap_or_default();
        if !seen.insert(key) {
            continue;
        }
        match link.order.as_ref().and_then(|id| facts.get(id)) {
            Some(fact) => tuples.push((
                link.order.clone(),
                Some(fact.owner_user_id.clone()),
                Some(fact.business_org_unit_id.clone()),
                row.id.clone(),
                row.version,
            )),
            None => tuples.push((None, None, None, row.id.clone(), row.version)),
        }
    }
    tuples
}

/// 发票进项分配关联元组；结算单来源与缺失订单计入未分配。
fn invoice_purchase_tuples(
    row: &InvoiceRow,
    links: &[PurchaseInvoiceLink],
    facts: &HashMap<String, LinkedPurchaseFact>,
) -> Vec<OrderTuple> {
    let mut seen = BTreeSet::new();
    let mut tuples = Vec::new();
    for link in links {
        let key = link.order.clone().unwrap_or_default();
        if !seen.insert(key) {
            continue;
        }
        match link.order.as_ref().and_then(|id| facts.get(id)) {
            Some(fact) => tuples.push((
                link.order.clone(),
                fact.owner_user_id.clone(),
                Some(fact.business_org_unit_id.clone()),
                row.id.clone(),
                row.version,
            )),
            None => tuples.push((None, None, None, row.id.clone(), row.version)),
        }
    }
    tuples
}

/// 发票双方向条件匹配：销售负责人验销项侧，采购负责人验进项侧，经办人验登记人。
fn matches_invoice_condition(
    sales: &[OrderTuple],
    purchase: &[OrderTuple],
    operators: &[String],
    condition: &FundsLinkedCondition,
) -> bool {
    if let Some(wanted) = &condition.operator_user_ids
        && !operators.iter().any(|id| wanted.iter().any(|item| item == id))
    {
        return false;
    }
    if let Some(wanted) = &condition.owner_user_ids
        && !sales
            .iter()
            .any(|(_, owner, _, _, _)| owner.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
    {
        return false;
    }
    if let Some(wanted) = &condition.secondary_operator_user_ids
        && !purchase
            .iter()
            .any(|(_, owner, _, _, _)| owner.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
    {
        return false;
    }
    if let Some(wanted) = &condition.org_unit_ids {
        let hit_sales = sales
            .iter()
            .any(|(_, _, org, _, _)| org.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)));
        let hit_purchase = purchase
            .iter()
            .any(|(_, _, org, _, _)| org.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)));
        if !hit_sales && !hit_purchase {
            return false;
        }
    }
    true
}

/// 发票业务归属筛选：来源销售单与应收子账只收窄授权结果。
fn matches_invoice_business(
    query: &erp_finance::dto::receivable::InvoiceListQuery,
    row: &InvoiceRow,
    sales: &[SalesInvoiceLink],
    sales_matched: &[String],
) -> bool {
    use erp_finance::entity::receivable::InvoiceDirection;
    if let Some(wanted) = query.sales_order_id.as_deref()
        && row.invoice_direction == InvoiceDirection::Sales
        && !sales.iter().any(|link| link.order.as_deref() == Some(wanted))
    {
        return false;
    }
    if let Some(wanted) = query.receivable_account_id.as_ref()
        && !sales.iter().any(|link| {
            link.view.receivable_account_id == wanted.to_string() && link.order.as_ref().is_none_or(|_| true)
        })
    {
        return false;
    }
    if query.sales_order_id.is_some() && row.invoice_direction == InvoiceDirection::Purchase {
        return sales_matched.is_empty() && sales.is_empty();
    }
    true
}

/// 发票整单口径求和；仅整单读取资格持有者可见的结果使用，不得用于部分授权。
fn sum_invoice_signed(sales: &[SalesInvoiceLink], purchase: &[PurchaseInvoiceLink]) -> Amount {
    let mut total = zero_amount();
    for link in sales {
        total = total.checked_add(link.signed);
    }
    for link in purchase {
        total = total.checked_add(link.signed);
    }
    total
}

/// 发票匹配份额求和；方向已在装载时按正反动作记入金额符号，不做差额推导。
fn sum_invoice_matched(
    sales: &[SalesInvoiceLink],
    purchase: &[PurchaseInvoiceLink],
    sales_matched: &[String],
    purchase_matched: &[String],
) -> Amount {
    let mut total = zero_amount();
    for link in sales {
        if link.order.as_ref().is_none_or(|order| sales_matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    for link in purchase {
        if link.order.as_ref().is_none_or(|order| purchase_matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    total
}

/// 发票汇总输入：匹配份额与未分配份额进入汇总，未授权订单份额不得进入。
fn invoice_summary_inputs(
    sales: &[SalesInvoiceLink],
    purchase: &[PurchaseInvoiceLink],
    sales_matched: &[String],
    purchase_matched: &[String],
) -> Vec<(String, Amount, LinkedOrderId)> {
    let mut inputs = Vec::new();
    for link in sales {
        if link.order.as_ref().is_none_or(|order| sales_matched.iter().any(|id| id == order)) {
            inputs.push((link.id.clone(), link.signed, link.order.clone()));
        }
    }
    for link in purchase {
        if link.order.as_ref().is_none_or(|order| purchase_matched.iter().any(|id| id == order)) {
            inputs.push((link.id.clone(), link.signed, link.order.clone()));
        }
    }
    inputs
}

/// 发票未分配余额沿用发票查询口径：蓝票含税减已分配，红票含税加已分配。
fn invoice_unallocated(row: &InvoiceRow, allocated: Amount) -> Amount {
    use erp_finance::entity::receivable::InvoiceKind;
    match row.invoice_kind {
        InvoiceKind::Blue => row.gross_amount.checked_sub(allocated),
        InvoiceKind::Red => row.gross_amount.checked_add(allocated),
    }
}

// __funds_scope_chunk_h__

use erp_finance::entity::receivable::SalesInvoiceRequest;

impl FundsAccess {
    /// 分页查询开票申请范围行：负责销售/申请人/当前开票处理人分别查询。
    pub async fn request_list_scoped(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        query.validate_scope_filters().map_err(Error::from)?;
        let page = query.page.unwrap_or(1).max(1);
        ensure_page(page, query.scope_version.as_deref())?;
        let snapshot = self.checked_requests(query, actor, query.scope_version.as_deref()).await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析开票申请详情动作；不可见与不存在统一为 NotFound。
    pub async fn request_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRequestRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_request_detail(&id, &actor, executor).await })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    async fn checked_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let snapshot = self.snapshot_requests(query, actor).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_requests(query, actor).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.load_requests(&query, &actor, executor).await })
            })
            .await
    }

    /// 分页查询开票申请范围行：申请人取创建人，处理人取工作项当前负责人。
    async fn load_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let (access, authorization) = self.resolve(actor, "sales_invoice_request", "list", executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "开票申请无可见范围"));
        }
        let candidates = self.page_all_requests(query, executor).await?;
        let decided = self.assemble_requests(query, candidates, &access, &authorization, executor).await?;
        let order_ids = decided.iter().map(|(row, _)| row.sales_order_id.to_string()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let order_nos = self.sales_order_nos(&order_ids, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, _) in decided.iter() {
            row.base.id.hash(&mut fingerprint);
            row.base.version.hash(&mut fingerprint);
            if let Some(order) = facts.get(&row.sales_order_id.to_string()) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_requests(query, decided, facts, order_nos, authorization, fingerprint, executor).await
    }

    /// 申请仓储按百行分页全量取回候选；超过上限整体拒绝，不得截断。
    async fn page_all_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceRequest>> {
        let mut items = Vec::new();
        let mut page_no = 1u64;
        loop {
            let mut paged = query.clone();
            paged.page = Some(page_no);
            paged.page_size = Some(100);
            let result = self.db.sales_invoice_requests().page(&paged, executor).await?;
            if result.total > 10_000 {
                return Err(Error::ValidationError("开票申请查询超过上限，请收窄组织或负责人条件".into()));
            }
            let done = result.items.len() < 100;
            items.extend(result.items);
            if done || items.len() as i64 >= result.total {
                break;
            }
            page_no += 1;
        }
        Ok(items)
    }

    /// 开票申请候选逐行判定可见性与筛选；缺失销售单的行跳过，不计未分配。
    async fn assemble_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        rows: Vec<SalesInvoiceRequest>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(SalesInvoiceRequest, Option<String>)>> {
        let order_ids = rows.iter().map(|row| row.sales_order_id.to_string()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let authorized = self.authorized_sales_ids(authorization, executor).await?;
        let allowed = authorized.map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let work_ids = rows.iter().filter_map(|row| row.work_item_id.clone()).collect::<Vec<_>>();
        let handlers = self.work_item_handlers(&work_ids, executor).await?;
        let condition = self.request_condition(query, executor).await?;
        let mut decided = Vec::new();
        for row in rows {
            let Some(fact) = facts.get(&row.sales_order_id.to_string()) else {
                continue;
            };
            if let Some(allowed) = &allowed
                && !allowed.contains(&row.sales_order_id.to_string())
            {
                continue;
            }
            let handler = row.work_item_id.as_ref().and_then(|id| handlers.get(id).cloned());
            let row_facts = FundsLinkedFacts {
                owner_user_id: Some(fact.owner_user_id.clone()),
                business_org_unit_id: Some(fact.business_org_unit_id.clone()),
                operator_user_ids: vec![row.created_by.clone()],
                secondary_operator_user_ids: handler.clone().into_iter().collect(),
                linked_document_id: row.sales_order_id.to_string(),
                linked_document_version: fact.version,
            };
            if !Self::allows(access, &row_facts)? {
                continue;
            }
            if !matches_linked_condition(&row_facts, &condition) {
                continue;
            }
            decided.push((row, handler));
        }
        Ok(decided)
    }

    /// 开票申请关联筛选条件：负责人、申请人、处理人与组织分别精确匹配。
    async fn request_condition(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().iter().cloned().collect::<Vec<_>>();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query
                .sales_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            operator_user_ids: query
                .applicant_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            secondary_operator_user_ids: query
                .handler_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            org_unit_ids,
        })
    }

    /// 关联销售单号一次取回；缺失单据的行已在装载阶段跳过。
    async fn sales_order_nos(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        use erp_core::ids::SalesOrderId;
        let mut map = HashMap::new();
        let mut unique = ids.to_vec();
        unique.sort();
        unique.dedup();
        for chunk in unique.chunks(500) {
            let keys = chunk.iter().map(|id| SalesOrderId::new(id.clone())).collect::<Vec<_>>();
            for order in self.db.sales_orders().find_orders_by_ids(&keys, executor).await? {
                map.insert(order.base.id.clone(), order.order_no.clone());
            }
        }
        Ok(map)
    }

    /// 开票申请候选分页裁剪与汇总装配；申请金额为行级事实，始终返回。
    async fn finish_requests(
        &self,
        query: &erp_finance::dto::receivable::InvoiceRequestQuery,
        decided: Vec<(SalesInvoiceRequest, Option<String>)>,
        facts: HashMap<String, LinkedSalesFact>,
        order_nos: HashMap<String, String>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedInvoiceRequestRow>> {
        let total = decided.len() as u64;
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let whole = authorization.whole();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, handler) in decided[start..end].iter() {
                let fact = facts.get(&row.sales_order_id.to_string());
                items.push(ScopedInvoiceRequestRow {
                    id: row.base.id.clone(),
                    request_no: row.request_no.clone(),
                    sales_order_id: row.sales_order_id.to_string(),
                    sales_order_no: order_nos
                        .get(&row.sales_order_id.to_string())
                        .cloned()
                        .unwrap_or_default(),
                    status: row.status,
                    created_at: row.base.created_at,
                    applicant_user_id: row.created_by.clone(),
                    handler_user_id: handler.clone(),
                    amount: row.data.amount,
                    permission_limited: !whole,
                    sales_owner_user_id: fact.map(|order| order.owner_user_id.clone()),
                    business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
                });
            }
        }
        let triples = decided
            .iter()
            .map(|(row, _)| (row.base.id.clone(), row.data.amount, Some(row.sales_order_id.to_string())))
            .collect::<Vec<_>>();
        let owner_of = facts
            .iter()
            .map(|(id, fact)| (id.clone(), fact.owner_user_id.clone()))
            .collect::<HashMap<_, _>>();
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(query.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = build_summary(&triples, &owner_of, None, &version, !whole)?;
        let owner_options = self.owner_options_sales(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "开票申请按关联销售当前负责人、申请人与当前开票处理人授权；不改变正式开票准入",
            ownership_basis: "linked_sales_owner_applicant_and_handler",
        })
    }

    /// 开票申请详情同一事务内解析、取数与裁剪；版本绑定关联销售单。
    async fn load_request_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedInvoiceRequestRow>> {
        let (access, authorization) =
            self.resolve(actor, "sales_invoice_request", "detail", executor).await?;
        let row = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let order_ids = vec![row.sales_order_id.to_string()];
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let fact = facts
            .get(&row.sales_order_id.to_string())
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let authorized = self.authorized_sales_ids(&authorization, executor).await?;
        if let Some(allowed) = &authorized
            && !allowed.contains(&row.sales_order_id.to_string())
        {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let handlers = self
            .work_item_handlers(&row.work_item_id.clone().into_iter().collect::<Vec<_>>(), executor)
            .await?;
        let handler = row.work_item_id.as_ref().and_then(|item| handlers.get(item).cloned());
        let row_facts = FundsLinkedFacts {
            owner_user_id: Some(fact.owner_user_id.clone()),
            business_org_unit_id: Some(fact.business_org_unit_id.clone()),
            operator_user_ids: vec![row.created_by.clone()],
            secondary_operator_user_ids: handler.clone().into_iter().collect(),
            linked_document_id: row.sales_order_id.to_string(),
            linked_document_version: fact.version,
        };
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let order_nos = self.sales_order_nos(&order_ids, executor).await?;
        let whole = authorization.whole();
        let data = ScopedInvoiceRequestRow {
            id: row.base.id.clone(),
            request_no: row.request_no.clone(),
            sales_order_id: row.sales_order_id.to_string(),
            sales_order_no: order_nos.get(&row.sales_order_id.to_string()).cloned().unwrap_or_default(),
            status: row.status,
            created_at: row.base.created_at,
            applicant_user_id: row.created_by.clone(),
            handler_user_id: handler,
            amount: row.data.amount,
            permission_limited: !whole,
            sales_owner_user_id: Some(fact.owner_user_id.clone()),
            business_org_unit_id: Some(fact.business_org_unit_id.clone()),
        };
        let parts = vec![format!("{}:{}", row.base.id, row.base.version), row_facts.version_part()];
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "开票申请按关联销售当前负责人、申请人与当前开票处理人授权；不改变正式开票准入",
            ownership_basis: "linked_sales_owner_applicant_and_handler",
        })
    }
}

// __funds_scope_chunk_i__

use erp_finance::repository::{PayableAccountFilter, PayableAccountRow};

impl FundsAccess {
    /// 分页查询应付往来子账范围行：来源采购单当前采购负责人查询。
    pub async fn payable_account_list_scoped(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_payable_accounts(params, &query, actor, purchase_access, params.scope_version.as_deref())
            .await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析应付子账详情动作；不可见与不存在统一为 NotFound。
    pub async fn payable_account_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedResult<ScopedPayableAccountRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_payable_account_detail(&id, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    async fn checked_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        let snapshot = self.snapshot_payable_accounts(params, query, actor, purchase_access).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_payable_accounts(params, query, actor, purchase_access).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_payable_accounts(&params, &query, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 分页查询应付子账范围行：采购来源按当前采购负责人，非采购来源按空事实。
    async fn load_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        use erp_finance::entity::payable::PayableSourceType;
        let (access, authorization) =
            self.resolve_with_purchase(actor, "payable_account", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "应付子账无可见范围"));
        }
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Payable,
        )
        .await?;
        let filter = PayableAccountFilter {
            source_document_id: query.source_document_id.clone(),
            keyword_ids,
            supplier_id: query.supplier_id.clone(),
            source_type: query.source_type,
            status: query.status,
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, application_core::SortDir::Asc),
        };
        let candidates = self.db.payable_accounts().search_payable_accounts(&filter, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("应付查询超过上限，请收窄组织或负责人条件".into()));
        }
        let po_ids = candidates
            .items
            .iter()
            .filter(|row| row.source_type == PayableSourceType::PurchaseOrder)
            .map(|row| row.source_document_id.clone())
            .collect::<Vec<_>>();
        let purchase_facts = self.purchase_fact_map(&po_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let condition = self.payable_account_condition(query, executor).await?;
        let mut decided = Vec::new();
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for row in candidates.items {
            let fact = (row.source_type == PayableSourceType::PurchaseOrder)
                .then(|| purchase_facts.get(&row.source_document_id))
                .flatten();
            if fact.is_some()
                && let Some(allowed) = &allowed
                && !allowed.contains(&row.source_document_id)
            {
                continue;
            }
            let row_facts = FundsLinkedFacts {
                owner_user_id: fact.and_then(|order| order.owner_user_id.clone()),
                business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
                operator_user_ids: Vec::new(),
                secondary_operator_user_ids: Vec::new(),
                linked_document_id: row.source_document_id.clone(),
                linked_document_version: fact.map(|order| order.version).unwrap_or(0),
            };
            if !Self::allows(&access, &row_facts)? {
                continue;
            }
            if !matches_linked_condition(&row_facts, &condition) {
                continue;
            }
            let whole = match fact {
                Some(_) => purchase_whole(&authorization),
                None => authorization.funds.is_company(),
            };
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            row.source_document_id.hash(&mut fingerprint);
            fact.map(|order| order.version).unwrap_or(0).hash(&mut fingerprint);
            decided.push((row, fact.cloned(), whole));
        }
        self.finish_payable_accounts(params, query, decided, authorization, fingerprint, executor).await
    }

    /// 应付子账关联筛选条件：采购负责人与组织分别精确匹配，只收窄授权结果。
    async fn payable_account_condition(
        &self,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().iter().cloned().collect::<Vec<_>>();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query
                .procurement_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            operator_user_ids: None,
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }

    /// 应付子账候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    async fn finish_payable_accounts(
        &self,
        params: &erp_finance::dto::payable::PayableAccountListParams,
        query: &erp_finance::dto::payable::PayableAccountListQuery,
        decided: Vec<(PayableAccountRow, Option<LinkedPurchaseFact>, bool)>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPayableAccountRow>> {
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = u64::from(query.paging.page_size).max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, fact, whole) in decided[start..end].iter() {
                items.push(cut_payable_account_row(row, fact.as_ref(), *whole));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        let mut all_whole = true;
        for (row, fact, whole) in decided.iter() {
            all_whole &= *whole;
            let order = fact.as_ref().map(|_| row.source_document_id.clone());
            triples.push((row.id.clone(), row.settled_total, order));
            if *whole {
                whole_sum = whole_sum.checked_add(row.gross_total);
            }
        }
        let owner_of = decided
            .iter()
            .filter_map(|(row, fact, _)| {
                fact.as_ref()
                    .and_then(|order| order.owner_user_id.clone())
                    .map(|owner| (row.source_document_id.clone(), owner))
            })
            .collect::<HashMap<_, _>>();
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary =
            build_summary(&triples, &owner_of, whole_amount(all_whole, whole_sum), &version, !all_whole)?;
        let owner_options = self.owner_options_purchase(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size: page_size as u32,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应付子账按来源采购单当前采购负责人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner",
        })
    }

    /// 应付子账详情同一事务内解析、取数与裁剪；版本绑定来源采购单。
    async fn load_payable_account_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedPayableAccountRow>> {
        use erp_core::ids::PayableAccountId;
        use erp_finance::entity::payable::PayableSourceType;
        let (access, authorization) =
            self.resolve_with_purchase(actor, "payable_account", "detail", purchase_access, executor).await?;
        let accounts = self
            .db
            .payable_accounts()
            .find_accounts_by_ids(&[PayableAccountId::new(id.to_string())], executor)
            .await?;
        let account =
            accounts.into_iter().next().ok_or_else(|| Error::NotFound("应付往来子账不存在".into()))?;
        let fact = if account.source_type == PayableSourceType::PurchaseOrder {
            self.purchase_fact_map(std::slice::from_ref(&account.source_document_id), executor)
                .await?
                .remove(&account.source_document_id)
        } else {
            None
        };
        if fact.is_some()
            && let Some(scope) = authorization.purchase_scope.as_ref()
        {
            let allowed = self.authorized_purchase_ids(purchase_access, scope, executor).await?;
            if allowed.is_some_and(|list| !list.contains(&account.source_document_id)) {
                return Err(Error::NotFound("应付往来子账不存在".into()));
            }
        }
        let row_facts = FundsLinkedFacts {
            owner_user_id: fact.as_ref().and_then(|order| order.owner_user_id.clone()),
            business_org_unit_id: fact.as_ref().map(|order| order.business_org_unit_id.clone()),
            operator_user_ids: Vec::new(),
            secondary_operator_user_ids: Vec::new(),
            linked_document_id: account.source_document_id.clone(),
            linked_document_version: fact.as_ref().map(|order| order.version).unwrap_or(0),
        };
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("应付往来子账不存在".into()));
        }
        let whole = match fact.as_ref() {
            Some(_) => purchase_whole(&authorization),
            None => authorization.funds.is_company(),
        };
        let data = cut_payable_account_row(
            &PayableAccountRow {
                id: account.base.id.clone(),
                stable: account.stable.clone(),
                source_document_id: account.source_document_id.clone(),
                supplier_id: account.supplier_id.to_string(),
                source_type: account.source_type,
                gross_total: account.gross_total,
                settled_total: account.settled_total,
                open_total: account.open_total,
                invoiceable_total: account.invoiceable_total,
                invoiced_total: account.invoiced_total,
                open_invoiceable_total: account.open_invoiceable_total,
                version: account.base.version,
                created_at: account.base.created_at,
            },
            fact.as_ref(),
            whole,
        );
        let parts = vec![format!("{}:{}", account.base.id, account.base.version), row_facts.version_part()];
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "应付子账按来源采购单当前采购负责人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner",
        })
    }
}

/// 应付子账单行裁剪；整单金额仅整单资格返回，否则为 null。
fn cut_payable_account_row(
    row: &PayableAccountRow,
    fact: Option<&LinkedPurchaseFact>,
    whole: bool,
) -> ScopedPayableAccountRow {
    ScopedPayableAccountRow {
        id: row.id.clone(),
        source_document_id: row.source_document_id.clone(),
        source_type: row.source_type,
        supplier_id: row.supplier_id.clone(),
        status: row.stable.status(),
        created_at: row.created_at,
        visible_settled_share: row.settled_total,
        gross_total: whole_amount(whole, row.gross_total),
        settled_total: whole_amount(whole, row.settled_total),
        open_total: whole_amount(whole, row.open_total),
        permission_limited: !whole,
        procurement_owner_user_id: fact.and_then(|order| order.owner_user_id.clone()),
        business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
    }
}

// __funds_scope_chunk_j__

use erp_finance::repository::{SupplierPaymentFilter, SupplierPaymentRow};

impl FundsAccess {
    /// 分页查询供应商付款范围行：采购负责人与付款经办人分别查询。
    pub async fn supplier_payment_list_scoped(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_supplier_payments(
                params,
                &query,
                actor,
                purchase_access,
                params.scope_version.as_deref(),
            )
            .await?;
        Ok(snapshot)
    }

    /// 独立详情重新解析付款详情动作；不可见与不存在统一为 NotFound。
    pub async fn supplier_payment_detail_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedResult<ScopedSupplierPaymentRow>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_supplier_payment_detail(&id, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    async fn checked_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let snapshot = self.snapshot_supplier_payments(params, query, actor, purchase_access).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current = self.snapshot_supplier_payments(params, query, actor, purchase_access).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_supplier_payments(&params, &query, &actor, &purchase_access, executor).await
                })
            })
            .await
    }

    /// 分页查询付款范围行：核销关联采购当前负责人与付款经办人分别查询。
    async fn load_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let (access, authorization) =
            self.resolve_with_purchase(actor, "supplier_payment", "list", purchase_access, executor).await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "供应商付款无可见范围"));
        }
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Payment,
        )
        .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids,
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: query.payment_no.clone(),
            supplier_id: query.supplier_id.clone(),
            status: query.status,
            page: 1,
            page_size: 10_000,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, application_core::SortDir::Asc),
        };
        let candidates = self.db.supplier_payments().search_supplier_payments(&filter, executor).await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("付款查询超过上限，请收窄组织或负责人条件".into()));
        }
        let decided = self
            .assemble_supplier_payments(
                query,
                candidates.items,
                &access,
                &authorization,
                purchase_access,
                executor,
            )
            .await?;
        let ids = decided.iter().map(|(row, _)| row.id.clone()).collect::<Vec<_>>();
        let links = self.payment_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        for (row, matched) in decided.iter() {
            row.id.hash(&mut fingerprint);
            row.version.hash(&mut fingerprint);
            for order in matched.iter().filter_map(|id| facts.get(id)) {
                order.version.hash(&mut fingerprint);
            }
        }
        self.finish_supplier_payments(
            params,
            query,
            decided,
            links,
            facts,
            authorization,
            fingerprint,
            executor,
        )
        .await
    }

    /// 付款经办人事实：创建与提交审计均计入；无过滤条件时不读审计。
    async fn payment_operators(
        &self,
        ids: &[String],
        want: bool,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<String>>> {
        if !want {
            return Ok(HashMap::new());
        }
        self.audit_operators(
            "supplier_payment",
            ids,
            |action| action == "supplier_payment.create" || action == "supplier_payment.commit",
            executor,
        )
        .await
    }

    /// 付款关联筛选条件：采购负责人、付款经办人与组织分别精确匹配。
    async fn payment_condition(
        &self,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().iter().cloned().collect::<Vec<_>>();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query
                .procurement_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            operator_user_ids: query
                .operator_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }

    /// 付款候选逐行判定可见性与筛选；授权集合外的关联份额不进入行与汇总。
    async fn assemble_supplier_payments(
        &self,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        rows: Vec<SupplierPaymentRow>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(SupplierPaymentRow, Vec<String>)>> {
        let ids = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let links = self.payment_matched_links(&ids, executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let operators = self.payment_operators(&ids, query.operator_user_ids.is_some(), executor).await?;
        let condition = self.payment_condition(query, executor).await?;
        let whole = purchase_whole(authorization);
        let mut decided = Vec::new();
        for row in rows {
            let empty_links: Vec<PaymentLink> = Vec::new();
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            let tuples = payment_tuples(&row, row_links, &facts);
            let matched = matched_orders(&tuples, &allowed);
            let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
            let visible = row_visible(access, &tuples, &doc_operators, &[])?;
            let unlinked = row_links.iter().all(|link| link.order.is_none());
            if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
                continue;
            }
            if !matches_multi_condition(&tuples, &doc_operators, &[], &condition) {
                continue;
            }
            decided.push((row, matched));
        }
        Ok(decided)
    }

    /// 付款候选分页裁剪与汇总装配；明细、汇总与导出复用同一已决集合。
    async fn finish_supplier_payments(
        &self,
        params: &erp_finance::dto::payable::SupplierPaymentListParams,
        query: &erp_finance::dto::payable::SupplierPaymentListQuery,
        decided: Vec<(SupplierPaymentRow, Vec<String>)>,
        links: HashMap<String, Vec<PaymentLink>>,
        facts: HashMap<String, LinkedPurchaseFact>,
        authorization: FundsAuthorization,
        fingerprint: std::collections::hash_map::DefaultHasher,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedSupplierPaymentRow>> {
        let total = decided.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = u64::from(query.paging.page_size).max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(decided.len());
        let whole = purchase_whole(&authorization);
        let empty_links: Vec<PaymentLink> = Vec::new();
        let mut items = Vec::new();
        if start < decided.len() {
            for (row, matched) in decided[start..end].iter() {
                let row_links = links.get(&row.id).unwrap_or(&empty_links);
                items.push(cut_payment_row(row, row_links, matched, whole));
            }
        }
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        for (row, matched) in decided.iter() {
            let row_links = links.get(&row.id).unwrap_or(&empty_links);
            triples.extend(payment_summary_inputs(row_links, matched));
            whole_sum = whole_sum.checked_add(row.amount);
        }
        let mut owner_of = HashMap::new();
        for (id, fact) in facts.iter() {
            if let Some(owner) = fact.owner_user_id.clone() {
                owner_of.insert(id.clone(), owner);
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary = build_summary(&triples, &owner_of, whole_amount(whole, whole_sum), &version, !whole)?;
        let owner_options = self.owner_options_purchase(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size: page_size as u32,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "付款按核销关联采购当前负责人与付款经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner_and_payment_operator",
        })
    }

    /// 付款详情同一事务内解析、取数与裁剪；版本绑定关联采购单。
    async fn load_supplier_payment_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedResult<ScopedSupplierPaymentRow>> {
        let (access, authorization) = self
            .resolve_with_purchase(actor, "supplier_payment", "detail", purchase_access, executor)
            .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids: Some(vec![id.to_string()]),
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.supplier_payments().search_supplier_payments(&filter, executor).await?;
        let row =
            page.items.into_iter().next().ok_or_else(|| Error::NotFound("供应商付款单不存在".into()))?;
        let links = self.payment_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<PaymentLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = payment_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = purchase_whole(&authorization);
        let operators = self.payment_operators(std::slice::from_ref(&row.id), true, executor).await?;
        let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
        let visible = row_visible(&access, &tuples, &doc_operators, &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("供应商付款单不存在".into()));
        }
        let data = cut_payment_row(&row, row_links, &matched, whole);
        let mut parts = vec![format!("{}:{}", row.id, row.version)];
        for order in matched.iter().filter_map(|id| facts.get(id)) {
            parts.push(format!("{}:{:?}:{}", id, order.owner_user_id, order.version));
        }
        Ok(FundsScopedResult {
            data,
            scope_version: scope_version(&authorization.context, &parts),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "付款按核销关联采购当前负责人与付款经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner_and_payment_operator",
        })
    }
}

/// 付款行关联元组；结算单来源与缺失订单的分配保留未分配归属，不丢份额。
fn payment_tuples(
    row: &SupplierPaymentRow,
    links: &[PaymentLink],
    facts: &HashMap<String, LinkedPurchaseFact>,
) -> Vec<OrderTuple> {
    let mut seen = BTreeSet::new();
    let mut tuples = Vec::new();
    for link in links {
        let key = link.order.clone().unwrap_or_default();
        if !seen.insert(key) {
            continue;
        }
        match link.order.as_ref().and_then(|id| facts.get(id)) {
            Some(fact) => tuples.push((
                link.order.clone(),
                fact.owner_user_id.clone(),
                Some(fact.business_org_unit_id.clone()),
                row.id.clone(),
                row.version,
            )),
            None => tuples.push((None, None, None, row.id.clone(), row.version)),
        }
    }
    tuples
}

/// 付款整单口径求和；仅整单读取资格持有者可见的结果使用，不得用于部分授权。
fn payment_sum_all(links: &[PaymentLink]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        total = total.checked_add(link.signed);
    }
    total
}

/// 付款匹配份额求和；无归属份额始终计入可见份额，不做差额推导。
fn payment_sum_signed(links: &[PaymentLink], matched: &[String]) -> Amount {
    let mut total = zero_amount();
    for link in links {
        if link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)) {
            total = total.checked_add(link.signed);
        }
    }
    total
}

/// 付款汇总输入：匹配份额与未分配份额进入汇总，未授权采购单份额不得进入。
fn payment_summary_inputs(links: &[PaymentLink], matched: &[String]) -> Vec<(String, Amount, LinkedOrderId)> {
    links
        .iter()
        .filter(|link| link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)))
        .map(|link| (link.id.clone(), link.signed, link.order.clone()))
        .collect()
}

/// 付款单行裁剪；整单金额与完整分配仅整单资格返回，否则为 null。
fn cut_payment_row(
    row: &SupplierPaymentRow,
    links: &[PaymentLink],
    matched: &[String],
    whole: bool,
) -> ScopedSupplierPaymentRow {
    let mut views: Vec<_> = if whole {
        links.iter().map(|link| link.view.clone()).collect()
    } else {
        links
            .iter()
            .filter(|link| link.order.as_ref().is_none_or(|order| matched.iter().any(|id| id == order)))
            .map(|link| link.view.clone())
            .collect()
    };
    views.sort_by_key(|view| view.allocation_seq);
    let net = payment_sum_all(links);
    ScopedSupplierPaymentRow {
        id: row.id.clone(),
        payment_no: row.payment_no.clone(),
        status: row.status,
        supplier_id: row.supplier_id.clone(),
        paid_at: row.paid_at,
        created_at: row.created_at,
        visible_allocated_share: payment_sum_signed(links, matched),
        amount: whole_amount(whole, row.amount),
        allocated_total: whole_amount(whole, net),
        unallocated_amount: whole_amount(whole, row.amount.checked_sub(net)),
        allocations: Some(views),
        permission_limited: !whole,
    }
}

// __funds_scope_chunk_k__

use erp_finance::entity::payable::PurchaseInvoiceAllocation;
use erp_finance::repository::PurchaseInvoiceAllocationFilter;

/// 进项发票分配的范围装配行：归属采购单、收票经办人与整单资格。
struct ScopedPurchaseAllocation {
    /// 分配实体。
    item: PurchaseInvoiceAllocation,
    /// 归属采购单；结算单来源与缺失时计入未分配。
    order: LinkedOrderId,
    /// 方向金额（正反动作已记符号）。
    signed: Amount,
    /// 进项发票号码。
    invoice_no: Option<String>,
    /// 装载时授权快照决定的本行整单读取资格。
    whole_flag: bool,
    /// 归属采购单当前负责人；无归属时为空，份额计入未分配。
    owner_name: Option<String>,
}

impl FundsAccess {
    /// 分页查询进项发票分配范围行：采购负责人与收票经办人分别查询（仅列表）。
    pub async fn purchase_invoice_allocation_list_scoped(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        params.validate()?;
        let query = params.normalized().map_err(Error::from)?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let snapshot = self
            .checked_purchase_invoice_allocations(
                params,
                &query,
                actor,
                purchase_access,
                params.scope_version.as_deref(),
            )
            .await?;
        Ok(snapshot)
    }

    /// 返回前重读授权及候选事实版本；变化时拒绝交付原结果。
    async fn checked_purchase_invoice_allocations(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        expected: Option<&str>,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        let snapshot =
            self.snapshot_purchase_invoice_allocations(params, query, actor, purchase_access).await?;
        if expected.is_some_and(|value| value != snapshot.scope_version) {
            return Err(changed());
        }
        let current =
            self.snapshot_purchase_invoice_allocations(params, query, actor, purchase_access).await?;
        if current.scope_version != snapshot.scope_version {
            return Err(changed());
        }
        Ok(snapshot)
    }

    /// 身份和业务事实均使用调用方同一事务，不缓存权限解析结果。
    async fn snapshot_purchase_invoice_allocations(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        let this = self.clone();
        let params = params.clone();
        let query = query.clone();
        let actor = actor.clone();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.load_purchase_invoice_allocations(
                        &params,
                        &query,
                        &actor,
                        &purchase_access,
                        executor,
                    )
                    .await
                })
            })
            .await
    }

    /// 分页查询进项发票分配范围行：分配按应付子账反查采购单，收票经办取发票登记人。
    async fn load_purchase_invoice_allocations(
        &self,
        params: &erp_finance::dto::payable::PurchaseInvoiceAllocationListParams,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<FundsScopedPage<ScopedPurchaseInvoiceAllocationRow>> {
        let (access, authorization) = self
            .resolve_with_purchase(actor, "purchase_invoice_allocation", "list", purchase_access, executor)
            .await?;
        if authorization.empty() {
            return Ok(empty_page(&authorization, "no_scope", "进项发票分配无可见范围"));
        }
        let filter = PurchaseInvoiceAllocationFilter {
            payable_account_id: query.payable_account_id.clone(),
            invoice_id: None,
            page: 1,
            page_size: 10_000,
            sort_ascending: false,
        };
        let candidates = self
            .db
            .purchase_invoice_allocations()
            .search_purchase_invoice_allocations(&filter, executor)
            .await?;
        if candidates.items.len() >= 10_000 {
            return Err(Error::ValidationError("收票查询超过上限，请收窄组织或负责人条件".into()));
        }
        let assembled = self
            .assemble_purchase_invoice_allocations(
                query,
                candidates.items,
                &access,
                &authorization,
                purchase_access,
                executor,
            )
            .await?;
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        authorization.context.scope_version.hash(&mut fingerprint);
        let mut triples = Vec::new();
        let mut whole_sum = zero_amount();
        let mut all_whole = true;
        let mut owner_of = HashMap::new();
        for scoped in assembled.iter() {
            scoped.item.base.id.hash(&mut fingerprint);
            scoped.item.base.version.hash(&mut fingerprint);
            triples.push((scoped.item.base.id.clone(), scoped.signed, scoped.order.clone()));
            if scoped.whole() {
                whole_sum = whole_sum.checked_add(scoped.signed);
            } else {
                all_whole = false;
            }
        }
        for (order, owner) in
            assembled.iter().filter_map(|scoped| scoped.order.clone().map(|id| (id, scoped.owner())))
        {
            if let Some(owner) = owner {
                owner_of.insert(order, owner);
            }
        }
        let total = assembled.len() as u64;
        let page = query.paging.page.max(1);
        let page_size = u64::from(query.paging.page_size).max(1);
        let start = ((page - 1) as usize).saturating_mul(page_size as usize);
        let end = start.saturating_add(page_size as usize).min(assembled.len());
        let mut items = Vec::new();
        if start < assembled.len() {
            for scoped in assembled[start..end].iter() {
                let whole = scoped.whole();
                items.push(ScopedPurchaseInvoiceAllocationRow {
                    id: scoped.item.base.id.clone(),
                    invoice_id: scoped.item.invoice_id.to_string(),
                    invoice_no: scoped.invoice_no.clone(),
                    payable_account_id: scoped.item.payable_account_id.to_string(),
                    created_at: scoped.item.base.created_at,
                    visible_allocated_amount: scoped.signed,
                    allocated_gross_amount: whole_amount(whole, scoped.signed),
                    permission_limited: !whole,
                });
            }
        }
        let version = format!("{:x}", fingerprint.finish());
        ensure_version(params.scope_version.as_deref(), &version).map_err(|_| changed())?;
        let summary =
            build_summary(&triples, &owner_of, whole_amount(all_whole, whole_sum), &version, !all_whole)?;
        let owner_options = self.owner_options_purchase(&authorization, executor).await?;
        Ok(FundsScopedPage {
            items,
            total,
            summary,
            owner_options,
            page,
            page_size: page_size as u32,
            scope_version: version.clone(),
            policy_version: authorization.context.policy_version,
            organization_version: authorization.context.organizations.version,
            as_of: authorization.context.as_of.as_utc().to_rfc3339(),
            empty_reason: None,
            scope_summary: "收票分配按应付子账来源采购当前负责人与收票经办人授权；部分授权仅返获授权份额",
            ownership_basis: "linked_purchase_owner_and_invoice_operator",
        })
    }

    /// 进项发票分配候选逐行判定可见性与筛选；授权集合外的份额不进入行与汇总。
    async fn assemble_purchase_invoice_allocations(
        &self,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        rows: Vec<PurchaseInvoiceAllocation>,
        access: &FundsResolvedScope,
        authorization: &FundsAuthorization,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ScopedPurchaseAllocation>> {
        use erp_core::ids::{InvoiceId, PayableAccountId};
        use erp_finance::entity::payable::PayableSourceType;
        let account_keys = rows
            .iter()
            .map(|item| PayableAccountId::new(item.payable_account_id.to_string()))
            .collect::<Vec<_>>();
        let accounts = self.db.payable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| {
                let order = (account.source_type == PayableSourceType::PurchaseOrder)
                    .then(|| account.source_document_id.clone());
                (account.base.id.clone(), order)
            })
            .collect::<HashMap<_, _>>();
        let invoice_keys =
            rows.iter().map(|item| InvoiceId::new(item.invoice_id.to_string())).collect::<Vec<_>>();
        let invoice_ids = invoice_keys.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let invoices = self.db.invoices().find_invoices_by_ids(&invoice_ids, executor).await?;
        let invoice_operator = invoices
            .into_iter()
            .map(|invoice| {
                (invoice.base.id.clone(), (invoice.stable.created_by.clone(), invoice.invoice_no.clone()))
            })
            .collect::<HashMap<_, _>>();
        let po_ids = account_order.values().filter_map(|order| order.clone()).collect::<Vec<_>>();
        let purchase_facts = self.purchase_fact_map(&po_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let condition = self.purchase_invoice_condition(query, executor).await?;
        let mut decided = Vec::new();
        for item in rows {
            let order = account_order.get(&item.payable_account_id.to_string()).cloned().flatten();
            let (operator, invoice_no) = invoice_operator
                .get(&item.invoice_id.to_string())
                .cloned()
                .map(|(operator, no)| (Some(operator), Some(no)))
                .unwrap_or((None, None));
            if let (Some(order), Some(allowed)) = (order.as_ref(), allowed.as_ref())
                && !allowed.contains(order)
            {
                continue;
            }
            let fact = order.as_ref().and_then(|id| purchase_facts.get(id));
            let row_facts = FundsLinkedFacts {
                owner_user_id: fact.and_then(|order| order.owner_user_id.clone()),
                business_org_unit_id: fact.map(|order| order.business_org_unit_id.clone()),
                operator_user_ids: operator.clone().into_iter().collect(),
                secondary_operator_user_ids: Vec::new(),
                linked_document_id: item.payable_account_id.to_string(),
                linked_document_version: item.base.version,
            };
            if !Self::allows(access, &row_facts)? {
                continue;
            }
            if !matches_linked_condition(&row_facts, &condition) {
                continue;
            }
            let signed = match item.allocation_action {
                PayableAllocationAction::Apply => item.allocated_gross_amount,
                PayableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
            };
            let whole = purchase_whole(authorization);
            decided.push(ScopedPurchaseAllocation {
                item,
                order,
                signed,
                invoice_no,
                whole_flag: whole,
                owner_name: fact.and_then(|order| order.owner_user_id.clone()),
            });
        }
        Ok(decided)
    }

    /// 收票分配关联筛选条件：采购负责人、收票经办人与组织分别精确匹配。
    async fn purchase_invoice_condition(
        &self,
        query: &erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery,
        executor: &mut dyn Executor,
    ) -> Result<FundsLinkedCondition> {
        let org_unit_ids = match &query.org_unit_ids {
            Some(ids) => {
                let list = ids.as_slice().iter().cloned().collect::<Vec<_>>();
                let expanded = self
                    .expand_org_units(&list, query.include_descendants.unwrap_or(false), executor)
                    .await?;
                Some(expanded.into_iter().collect::<Vec<_>>())
            },
            None => None,
        };
        Ok(FundsLinkedCondition {
            owner_user_ids: query
                .procurement_owner_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            operator_user_ids: query
                .operator_user_ids
                .as_ref()
                .map(|ids| ids.as_slice().iter().cloned().collect()),
            secondary_operator_user_ids: None,
            org_unit_ids,
        })
    }
}

impl ScopedPurchaseAllocation {
    /// 本行整单读取资格由调用时授权快照决定，装载后不再重算。
    fn whole(&self) -> bool {
        self.whole_flag
    }

    /// 归属采购单当前负责人；结算单来源与缺失时为空，份额计入未分配。
    fn owner(&self) -> Option<String> {
        self.owner_name.clone()
    }
}

// __funds_scope_chunk_l__

impl FundsAccess {
    /// 命令守卫：同一事务内按详情动作独立解析并重验责任新鲜度。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `resource` - 资金资源（仅读动作已注册的七资源）
    /// * `id` - 命令目标单据主键
    /// * `purchase_access` - 采购关联资源必填的组合层采购访问器
    /// * `executor` - 命令原事务执行器，不得另开事务
    ///
    /// # 返回
    /// 可见时成功；不可见与不存在统一为 NotFound，未注册命令资源为 Forbidden。
    ///
    /// # 错误
    /// 授权解析、事实读取或版本校验失败时拒绝命令。
    ///
    /// # 关键业务约束
    /// 正式审批准入不变；本守卫只做读包含与责任新鲜度重验，不授予审批资格。
    pub async fn guard_funds_command(
        &self,
        actor: &AuditActor,
        resource: &str,
        id: &str,
        purchase_access: Option<&PurchaseAccess>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match resource {
            "receivable_account" => self.guard_receivable_account(actor, id, executor).await,
            "customer_receipt" => self.guard_customer_receipt(actor, id, executor).await,
            "invoice" => {
                let access =
                    purchase_access.ok_or_else(|| Error::Forbidden("发票命令缺少采购访问器".into()))?;
                self.guard_invoice(actor, id, access, executor).await
            },
            "sales_invoice_request" => self.guard_request(actor, id, executor).await,
            "payable_account" => {
                let access =
                    purchase_access.ok_or_else(|| Error::Forbidden("应付命令缺少采购访问器".into()))?;
                self.guard_payable_account(actor, id, access, executor).await
            },
            "supplier_payment" => {
                let access =
                    purchase_access.ok_or_else(|| Error::Forbidden("付款命令缺少采购访问器".into()))?;
                self.guard_supplier_payment(actor, id, access, executor).await
            },
            _ => Err(Error::Forbidden("该资源不支持资金命令守卫".into())),
        }
    }

    /// 新建命令守卫：关联销售/采购单必须在操作人当前授权集合内。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `sales_order_ids` - 新单据关联的销售单主键集合
    /// * `purchase_order_ids` - 新单据关联的采购单主键集合
    /// * `purchase_access` - 含采购关联时必填的组合层采购访问器
    /// * `executor` - 命令原事务执行器
    ///
    /// # 返回
    /// 全部关联单据均在授权集合内时成功，否则为 Forbidden。
    pub async fn guard_funds_create(
        &self,
        actor: &AuditActor,
        sales_order_ids: &[String],
        purchase_order_ids: &[String],
        purchase_access: Option<&PurchaseAccess>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !sales_order_ids.is_empty() {
            let (_, authorization) = self.resolve(actor, "receivable_account", "list", executor).await?;
            let allowed = self.authorized_sales_ids(&authorization, executor).await?;
            if let Some(allowed) = allowed
                && !sales_order_ids.iter().all(|id| allowed.iter().any(|item| item == id))
            {
                return Err(Error::Forbidden("关联销售单超出当前数据范围".into()));
            }
        }
        if !purchase_order_ids.is_empty() {
            let access = purchase_access.ok_or_else(|| Error::Forbidden("新建命令缺少采购访问器".into()))?;
            let (_, authorization) =
                self.resolve_with_purchase(actor, "payable_account", "list", access, executor).await?;
            let scope = authorization.purchase_scope.clone().unwrap_or_default();
            let allowed = self.authorized_purchase_ids(access, &scope, executor).await?;
            if let Some(allowed) = allowed
                && !purchase_order_ids.iter().all(|id| allowed.iter().any(|item| item == id))
            {
                return Err(Error::Forbidden("关联采购单超出当前数据范围".into()));
            }
        }
        Ok(())
    }

    /// 应收子账命令守卫：详情动作重验关联销售当前负责人与登记经办。
    async fn guard_receivable_account(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, _) = self.resolve(actor, "receivable_account", "detail", executor).await?;
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&account.sales_order_id.to_string(), executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        let facts = FundsLinkedFacts {
            owner_user_id: Some(order.sales_owner_user_id.clone()),
            business_org_unit_id: Some(order.business_org_unit_id.clone()),
            operator_user_ids: vec![account.stable.created_by.clone()],
            secondary_operator_user_ids: Vec::new(),
            linked_document_id: order.base.id.clone(),
            linked_document_version: order.base.version,
        };
        if !Self::allows(&access, &facts)? {
            return Err(Error::NotFound("应收往来子账不存在".into()));
        }
        Ok(())
    }

    /// 回款命令守卫：详情动作重验核销关联销售与经办人新鲜度。
    async fn guard_customer_receipt(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use erp_finance::repository::CustomerReceiptFilter;
        let (access, authorization) = self.resolve(actor, "customer_receipt", "detail", executor).await?;
        let filter = CustomerReceiptFilter {
            keyword_ids: None,
            receipt_ids: Some(vec![id.to_string()]),
            pending_entry_ids: Vec::new(),
            receipt_no: None,
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.customer_receipts().search_customer_receipts(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
        let links = self.receipt_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.sales_fact_map(&order_ids, executor).await?;
        let allowed = self
            .authorized_sales_ids(&authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<ReceiptLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = receipt_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = authorization.whole();
        let visible = row_visible(&access, &tuples, &[], &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("客户回款单不存在".into()));
        }
        Ok(())
    }

    /// 发票命令守卫：双方向详情动作重验分配关联与登记人新鲜度。
    async fn guard_invoice(
        &self,
        actor: &AuditActor,
        id: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use erp_finance::entity::receivable::InvoiceDirection;
        use erp_finance::repository::InvoiceFilter;
        let (access, authorization) =
            self.resolve_dual(actor, "invoice", "detail", purchase_access, executor).await?;
        let filter = InvoiceFilter {
            keyword_ids: None,
            invoice_ids: Some(vec![id.to_string()]),
            invoice_direction: None,
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.invoices().search_invoices(&filter, executor).await?;
        let row = page.items.into_iter().next().ok_or_else(|| Error::NotFound("发票不存在".into()))?;
        let sales_links = self.sales_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let purchase_links =
            self.purchase_invoice_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let (sales_facts, purchase_facts) =
            self.invoice_fact_maps(&sales_links, &purchase_links, executor).await?;
        let empty_sales: Vec<SalesInvoiceLink> = Vec::new();
        let empty_purchase: Vec<PurchaseInvoiceLink> = Vec::new();
        let sales = sales_links.get(&row.id).unwrap_or(&empty_sales);
        let purchase = purchase_links.get(&row.id).unwrap_or(&empty_purchase);
        let mut combined = invoice_sales_tuples(&row, sales, &sales_facts);
        combined.extend(invoice_purchase_tuples(&row, purchase, &purchase_facts));
        let sales_allowed = self
            .authorized_sales_ids(&authorization, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let purchase_allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let sales_part: Vec<OrderTuple> = invoice_sales_tuples(&row, sales, &sales_facts);
        let purchase_part: Vec<OrderTuple> = invoice_purchase_tuples(&row, purchase, &purchase_facts);
        let matched_any = !matched_orders(&sales_part, &sales_allowed).is_empty()
            || !matched_orders(&purchase_part, &purchase_allowed).is_empty();
        let whole = match row.invoice_direction {
            InvoiceDirection::Sales => authorization.whole(),
            InvoiceDirection::Purchase => purchase_whole(&authorization),
        };
        let operators = vec![row.stable.created_by.clone()];
        let visible = row_visible(&access, &combined, &operators, &[])?;
        let unlinked = combined.iter().all(|(order, _, _, _, _)| order.is_none());
        let empty = sales.is_empty() && purchase.is_empty();
        if !keep_row(visible, whole, matched_any, empty || unlinked) {
            return Err(Error::NotFound("发票不存在".into()));
        }
        Ok(())
    }

    /// 开票申请命令守卫：详情动作重验关联销售、申请人与处理人新鲜度。
    async fn guard_request(&self, actor: &AuditActor, id: &str, executor: &mut dyn Executor) -> Result<()> {
        let (access, authorization) =
            self.resolve(actor, "sales_invoice_request", "detail", executor).await?;
        let row = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let facts = self.sales_fact_map(&[row.sales_order_id.to_string()], executor).await?;
        let fact = facts
            .get(&row.sales_order_id.to_string())
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        let allowed = self.authorized_sales_ids(&authorization, executor).await?;
        if allowed.is_some_and(|list| !list.contains(&row.sales_order_id.to_string())) {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        let handlers = self
            .work_item_handlers(&row.work_item_id.clone().into_iter().collect::<Vec<_>>(), executor)
            .await?;
        let row_facts = FundsLinkedFacts {
            owner_user_id: Some(fact.owner_user_id.clone()),
            business_org_unit_id: Some(fact.business_org_unit_id.clone()),
            operator_user_ids: vec![row.created_by.clone()],
            secondary_operator_user_ids: row
                .work_item_id
                .as_ref()
                .and_then(|item| handlers.get(item).cloned())
                .into_iter()
                .collect(),
            linked_document_id: row.sales_order_id.to_string(),
            linked_document_version: fact.version,
        };
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("开票申请不存在".into()));
        }
        Ok(())
    }

    /// 应付子账命令守卫：详情动作重验来源采购当前负责人新鲜度。
    async fn guard_payable_account(
        &self,
        actor: &AuditActor,
        id: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use erp_core::ids::PayableAccountId;
        use erp_finance::entity::payable::PayableSourceType;
        let (access, authorization) =
            self.resolve_with_purchase(actor, "payable_account", "detail", purchase_access, executor).await?;
        let accounts = self
            .db
            .payable_accounts()
            .find_accounts_by_ids(&[PayableAccountId::new(id.to_string())], executor)
            .await?;
        let account =
            accounts.into_iter().next().ok_or_else(|| Error::NotFound("应付往来子账不存在".into()))?;
        let fact = if account.source_type == PayableSourceType::PurchaseOrder {
            self.purchase_fact_map(std::slice::from_ref(&account.source_document_id), executor)
                .await?
                .remove(&account.source_document_id)
        } else {
            None
        };
        if fact.is_some()
            && let Some(scope) = authorization.purchase_scope.as_ref()
        {
            let allowed = self.authorized_purchase_ids(purchase_access, scope, executor).await?;
            if allowed.is_some_and(|list| !list.contains(&account.source_document_id)) {
                return Err(Error::NotFound("应付往来子账不存在".into()));
            }
        }
        let row_facts = FundsLinkedFacts {
            owner_user_id: fact.as_ref().and_then(|order| order.owner_user_id.clone()),
            business_org_unit_id: fact.as_ref().map(|order| order.business_org_unit_id.clone()),
            operator_user_ids: Vec::new(),
            secondary_operator_user_ids: Vec::new(),
            linked_document_id: account.source_document_id.clone(),
            linked_document_version: fact.as_ref().map(|order| order.version).unwrap_or(0),
        };
        if !Self::allows(&access, &row_facts)? {
            return Err(Error::NotFound("应付往来子账不存在".into()));
        }
        Ok(())
    }

    /// 付款命令守卫：详情动作重验核销关联采购与付款经办新鲜度。
    async fn guard_supplier_payment(
        &self,
        actor: &AuditActor,
        id: &str,
        purchase_access: &PurchaseAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, authorization) = self
            .resolve_with_purchase(actor, "supplier_payment", "detail", purchase_access, executor)
            .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids: Some(vec![id.to_string()]),
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let page = self.db.supplier_payments().search_supplier_payments(&filter, executor).await?;
        let row =
            page.items.into_iter().next().ok_or_else(|| Error::NotFound("供应商付款单不存在".into()))?;
        let links = self.payment_matched_links(std::slice::from_ref(&row.id), executor).await?;
        let order_ids = links.values().flatten().filter_map(|link| link.order.clone()).collect::<Vec<_>>();
        let facts = self.purchase_fact_map(&order_ids, executor).await?;
        let purchase_scope = authorization.purchase_scope.clone().unwrap_or_default();
        let allowed = self
            .authorized_purchase_ids(purchase_access, &purchase_scope, executor)
            .await?
            .map(|list| list.into_iter().collect::<BTreeSet<_>>());
        let empty_links: Vec<PaymentLink> = Vec::new();
        let row_links = links.get(&row.id).unwrap_or(&empty_links);
        let tuples = payment_tuples(&row, row_links, &facts);
        let matched = matched_orders(&tuples, &allowed);
        let whole = purchase_whole(&authorization);
        let operators = self.payment_operators(std::slice::from_ref(&row.id), true, executor).await?;
        let doc_operators = operators.get(&row.id).cloned().unwrap_or_default();
        let visible = row_visible(&access, &tuples, &doc_operators, &[])?;
        let unlinked = row_links.iter().all(|link| link.order.is_none());
        if !keep_row(visible, whole, !matched.is_empty(), row_links.is_empty() || unlinked) {
            return Err(Error::NotFound("供应商付款单不存在".into()));
        }
        Ok(())
    }
}
