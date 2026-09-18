//! 资金授权解析与范围版本守卫。

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::ports::funds_scope::{FundsDataScopePort, FundsResolvedClause, FundsResolvedScope};
use erp_identity::access_control::{ResolvedScope, ScopeClause, ScopedObject};
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
    pub(super) fingerprint: std::collections::hash_map::DefaultHasher,
    pub(super) no_scope: bool,
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
        let (sales_access, sales) = self.linked_sales_scope(actor, &access, executor).await?;
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
            self.linked_purchase_scope(actor, purchase_access, executor).await?;
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
    pub(super) async fn linked_sales_scope(
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
    pub(super) async fn linked_purchase_scope(
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
pub(super) fn purchase_clause(clause: &erp_procurement::ports::PurchaseResolvedClause) -> ScopeClause {
    ScopeClause {
        company: clause.company,
        self_owned: clause.self_owned,
        collaborative: false,
        org_unit_ids: clause.org_unit_ids.iter().cloned().collect(),
        ..ScopeClause::default()
    }
}

/// 把资金 Port 事实转为销售条件编译可用的已解析上下文。
pub(super) fn access_output(access: &FundsResolvedScope) -> Result<AuthorizedDataScope> {
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
pub(super) fn organization_version(version: u64) -> OrganizationState {
    OrganizationState { version, ..Default::default() }
}

impl FundsLinkedFacts {
    /// 判断当前操作人是否为关联单据的当前负责人。
    pub(super) fn owner_is(&self, user: &str) -> bool {
        self.owner_user_id.as_deref() == Some(user)
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
pub fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    crate::support::ensure_deep_page(page, version)
}

/// 版本不一致时返回可识别的范围变化错误并要求从第一页刷新。
pub fn ensure_version(expected: Option<&str>, actual: &str) -> Result<()> {
    if expected.is_some_and(|version| version != actual) {
        return Err(crate::support::data_scope_changed("数据范围已变化，请从第一页刷新"));
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
/// 空范围保持空集且版本可跨页回传，不得补公司范围。
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

    #[test]
    pub(super) fn whole_document_rules_require_both_funds_and_linked_company() {
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
