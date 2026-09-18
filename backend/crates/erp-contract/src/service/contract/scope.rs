//! 合同列表与候选的一致授权快照；范围与业务版本跨页携带。

use application_core::AuditActor;
use persistence_core::Transactional;

use super::ContractService;
use super::access::intersect_ids;
use crate::dto::contract::{ContractListQuery, ContractListScope, ContractListView, PageView};
use crate::error::{Error, Result};
use crate::ports::{ContractDataScopePort, ContractResolvedScope};
use crate::repository::list_search::ContractSearch;
use crate::repository::prelude::*;
use crate::repository::scope::ContractReadScope;
use crate::repository::{ContractExt, ContractFilter};

/// 同一事务内的列表快照，供跨页版本复核。
pub(super) struct ContractSnapshot {
    /// 对外列表视图。
    pub view: ContractListView,
    /// 身份授权上下文。
    pub context: ContractResolvedScope,
    /// 授权集合本身为空。
    pub no_scope: bool,
}

impl ContractService {
    /// 授权、总数、候选与合同版本全部在同一个事务读取。
    ///
    /// # 参数
    /// * `query` - 已归一化查询
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的列表快照。
    ///
    /// # 错误
    /// 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    ///
    /// # 关键业务约束
    /// 组织筛选按当前主负责人所属组织收窄，不得扩大授权结果。
    pub(super) async fn list_snapshot(
        &self,
        query: ContractListQuery,
        actor: &AuditActor,
    ) -> Result<ContractSnapshot> {
        let db = self.db.clone();
        let access = self.access();
        let data_scope = self.data_scope.clone();
        let assignments = self.assignments.clone();
        let accounts = self.accounts.clone();
        let customers = self.customers.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (context, scope, no_scope, as_of) =
                        resolve_list_scope(&access, &actor, executor).await?;
                    let (filter, search) = load_list_filter_and_search(
                        &db,
                        data_scope.as_ref(),
                        assignments.as_ref(),
                        customers.as_ref(),
                        accounts.as_ref(),
                        &context,
                        &scope,
                        &query,
                        as_of,
                        executor,
                    )
                    .await?;
                    finish_list_snapshot(&db, accounts.as_ref(), filter, search, context, executor)
                        .await
                        .map(|(view, context)| ContractSnapshot { view, context, no_scope })
                })
            })
            .await
    }
}

/// 解析列表授权范围与归属时点（`list_snapshot` 第一步）。
///
/// # 参数
/// * `access` - 合同范围访问器
/// * `actor` - 已认证操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回已解析事实、读取范围、空范围标记与归属自然日。
///
/// # 错误
/// 无动作权限或组织关系非法时拒绝。
async fn resolve_list_scope(
    access: &super::access::ContractAccess,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(ContractResolvedScope, ContractReadScope, bool, erp_core::common::time::BusinessDate)> {
    let (context, scope) = access.resolve(actor, "list", executor).await?;
    let no_scope = !context.has_scope_rules();
    let as_of = super::access::business_date(context.as_of)?;
    Ok((context, scope, no_scope, as_of))
}

/// 求交授权与业务筛选并装配搜索事实（`list_snapshot` 第二步）。
///
/// # 参数
/// * `db` - 合同数据库
/// * `data_scope` - 组织展开与成员事实 Port
/// * `assignments` - 当前主责事实
/// * `customers` - 客户编号事实
/// * `accounts` - 账号显示名 Port
/// * `context` - 已解析的合同范围事实
/// * `scope` - 已映射的合同范围
/// * `query` - 归一化查询
/// * `as_of` - 客户归属自然日
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回仓储筛选与搜索条件。
///
/// # 错误
/// 组织展开失败、成员超限或归属查询失败时拒绝。
#[allow(clippy::too_many_arguments)]
async fn load_list_filter_and_search(
    db: &mongodb::Database,
    data_scope: &dyn ContractDataScopePort,
    assignments: &dyn crate::ports::CustomerAssignmentFactsPort,
    customers: &dyn crate::ports::CustomerFactsPort,
    accounts: &dyn crate::ports::AccountNamePort,
    context: &ContractResolvedScope,
    scope: &ContractReadScope,
    query: &ContractListQuery,
    as_of: erp_core::common::time::BusinessDate,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(ContractFilter, ContractSearch)> {
    let (customer_ids, historical_contract_ids) =
        apply_list_filters(db, data_scope, assignments, context, scope, query, executor).await?;
    let filter = list_filter(query, customer_ids, historical_contract_ids);
    let ids = db.contract().list_customer_ids(&filter, executor).await?;
    let customer_facts =
        super::query::list_customer_facts_with(customers, assignments, accounts, &ids, as_of, executor)
            .await?;
    let search = ContractSearch {
        q: query.q.clone(),
        metric: query.metric,
        settlement_party_id: query.settlement_party_id.clone(),
        owner_user_ids: query.owner_user_ids.clone(),
        customers: customer_facts,
    };
    Ok((filter, search))
}

/// 执行搜索聚合并组装对外视图（`list_snapshot` 第三步）。
///
/// # 参数
/// * `db` - 合同数据库
/// * `accounts` - 账号显示名 Port
/// * `filter` - 已与授权求交的仓储筛选
/// * `search` - 列表搜索条件
/// * `context` - 已解析事实（指纹写入范围版本）
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回对外视图与携带指纹的授权上下文。
///
/// # 错误
/// 聚合失败、版本超限或候选读取失败时拒绝。
async fn finish_list_snapshot(
    db: &mongodb::Database,
    accounts: &dyn crate::ports::AccountNamePort,
    filter: ContractFilter,
    search: ContractSearch,
    mut context: ContractResolvedScope,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(ContractListView, ContractResolvedScope)> {
    let result = db.contract().search_list(&filter, &search, executor).await?;
    let versions = db.contract().query_versions(&filter, executor).await?;
    if versions.len() > 10_000 {
        return Err(Error::ValidationError("合同查询超过上限，请收窄组织或负责人条件".into()));
    }
    let fingerprint = super::access::scope_fingerprint_input(&[], &[], versions.as_slice(), &[]);
    context.scope_version =
        format!("{}:{:x}", context.scope_version, super::access::stable_fingerprint(&fingerprint));
    let owner_options = accounts
        .filter_options(&search.customers.iter().filter_map(|c| c.owner_id.clone()).collect::<Vec<_>>())
        .await?
        .into_iter()
        .map(|o| crate::dto::contract::ContractFilterOption { value: o.value, label: o.label })
        .collect();
    let total = result.total();
    let items = super::query::contract_rows(db, result.items, &search.customers, executor).await?;
    let view = ContractListView {
        ownership_basis: "current_customer_owner",
        scope_version: String::new(),
        policy_version: 0,
        organization_version: 0,
        as_of: String::new(),
        empty_reason: None,
        scope_summary: "合同当前客户主负责人、协作关系、负责人所属组织及合法单据参与",
        page: PageView { items, total, page: filter.page, page_size: filter.page_size },
        metrics: result.metrics.into_iter().next().unwrap_or_default(),
        settlement_options: result.settlement_options,
        owner_options,
    };
    Ok((view, context))
}

/// 将内部快照转换为对外列表视图。
///
/// # 参数
/// * `snapshot` - 同一事务读取的授权与业务快照
///
/// # 返回
/// 返回可序列化的列表响应。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不把内部授权证明或全量人员集合返回给客户端。
pub(super) fn to_list_view(snapshot: ContractSnapshot) -> ContractListView {
    let mut view = snapshot.view;
    view.scope_version = snapshot.context.scope_version;
    view.policy_version = snapshot.context.policy_version;
    view.organization_version = snapshot.context.organization_version;
    view.as_of = snapshot.context.as_of.as_utc().to_rfc3339();
    view.empty_reason = snapshot.no_scope.then_some("no_scope");
    view
}

/// 构造可被 HTTP 边界识别的范围变化冲突。
///
/// # 参数
/// * `detail` - 面向用户的中文恢复说明
///
/// # 返回
/// 返回带 `DATA_SCOPE_CHANGED` 前缀的冲突错误。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 稳定码必须出现在错误载荷中，供 HTTP 映射为独立 `code`，不得只依赖展示文案。
pub(super) fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

/// 后续页必须携带当前范围版本，禁止拼接不同授权快照。
///
/// # 参数
/// * `page` - 请求页码
/// * `version` - 客户端回传的范围版本
///
/// # 返回
/// 第一页或版本非空时成功。
///
/// # 错误
/// 第二页及之后缺少版本时返回 `DATA_SCOPE_CHANGED`。
///
/// # 关键业务约束
/// 不得在缺版本时继续查询并默默使用新授权。
pub(super) fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    if page > 1 && version.is_none_or(str::is_empty) {
        return Err(data_scope_changed("请从第一页刷新后继续查询"));
    }
    Ok(())
}

/// 客户端回传的范围版本必须与当前快照一致。
///
/// # 参数
/// * `expected` - 后续页携带的范围版本；第一页可为空
/// * `actual` - 本次查询快照的范围版本
///
/// # 返回
/// 未携带或完全一致时成功。
///
/// # 错误
/// 版本不一致时返回 `DATA_SCOPE_CHANGED`。
///
/// # 关键业务约束
/// 不得把新旧授权结果拼接成同一列表。
pub(super) fn ensure_scope_version(expected: Option<&str>, actual: &str) -> Result<()> {
    if expected.is_some_and(|value| value != actual) {
        return Err(data_scope_changed("数据范围已变化，请从第一页刷新"));
    }
    Ok(())
}

/// 同一查询内两次快照的范围版本必须一致。
///
/// # 参数
/// * `first` - 首次快照版本
/// * `second` - 复核快照版本
///
/// # 返回
/// 两次版本相同时成功。
///
/// # 错误
/// 查询过程中范围或合同资料变化时返回 `DATA_SCOPE_CHANGED`。
///
/// # 关键业务约束
/// 复核失败必须整页拒绝，不得返回半新半旧结果。
pub(super) fn ensure_stable_snapshot(first: &str, second: &str) -> Result<()> {
    if first != second {
        return Err(data_scope_changed("数据范围或合同资料已变化，请刷新"));
    }
    Ok(())
}

/// 归一化查询与权限客户集合求交后的仓储条件；空集合必须保留。
///
/// # 参数
/// * `query` - 归一化查询
/// * `customer_ids` - 已与筛选求交的授权客户
/// * `historical_contract_ids` - 仍受筛选约束的历史参与合同
///
/// # 返回
/// 返回仓储筛选。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得把 `None` 解释为未授权全量。
fn list_filter(
    query: &ContractListQuery,
    customer_ids: Option<Vec<String>>,
    historical_contract_ids: Vec<String>,
) -> ContractFilter {
    ContractFilter {
        contract_no: query.contract_no.clone(),
        customer_id: None,
        customer_ids,
        historical_contract_ids,
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, crate::dto::contract::SortDir::Asc),
    }
}

/// 将业务筛选与授权集合求交。
///
/// # 参数
/// * `data_scope` - 组织展开与成员事实 Port
/// * `assignments` - 当前主责事实
/// * `context` - 已解析的合同范围事实
/// * `scope` - 已映射的合同范围
/// * `query` - 归一化查询
/// * `as_of` - 客户归属自然日
/// * `executor` - 调用方执行器
///
/// # 返回
/// 授权客户集合与仍可见的历史参与合同。
///
/// # 错误
/// 组织展开失败、成员超限或归属查询失败时拒绝。
///
/// # 关键业务约束
/// Assigned 只是收窄条件；All 不得绕过 DataScope。
async fn apply_list_filters(
    db: &mongodb::Database,
    data_scope: &dyn ContractDataScopePort,
    assignments: &dyn crate::ports::CustomerAssignmentFactsPort,
    context: &ContractResolvedScope,
    scope: &ContractReadScope,
    query: &ContractListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(Option<Vec<String>>, Vec<String>)> {
    let as_of = super::access::business_date(context.as_of)?;
    let mut ids = scope.authorized_customer_ids.clone();
    let mut constraint: Option<Vec<String>> = None;
    constraint = intersect_ids(constraint, assignment_filter(scope, query.scope));
    if let Some(owners) = &query.owner_user_ids {
        constraint = Some(
            assignments
                .current_owner_customer_ids(constraint.as_deref(), Some(owners.as_slice()), as_of, executor)
                .await?,
        );
    }
    constraint =
        apply_org_unit_filter(data_scope, assignments, context, constraint, query, as_of, executor).await?;
    if let Some(customer_id) = &query.customer_id {
        constraint = intersect_ids(constraint, Some(vec![customer_id.clone()]));
    }
    ids = intersect_ids(ids, constraint.clone());
    let history = narrow_history(
        db,
        scope.historical_contract_ids.clone(),
        constraint.as_deref(),
        ids.is_none() && constraint.is_none(),
        executor,
    )
    .await?;
    Ok((ids, history))
}

/// 按请求筛选收窄历史参与合同。
///
/// # 参数
/// * `db` - 合同集合
/// * `history` - 已证明的历史参与合同
/// * `constraint` - 请求侧客户筛选；`None` 表示无额外筛选
/// * `company_unfiltered` - 公司范围且无筛选时历史已被全量覆盖
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回仍应并入读取条件的历史合同 ID。
///
/// # 错误
/// 读取合同客户映射失败时拒绝。
///
/// # 关键业务约束
/// 筛选只能收窄；公司范围不必再并入历史 ID。
async fn narrow_history(
    db: &mongodb::Database,
    history: Vec<String>,
    constraint: Option<&[String]>,
    company_unfiltered: bool,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<String>> {
    if company_unfiltered || history.is_empty() {
        return Ok(Vec::new());
    }
    let Some(customers) = constraint else {
        return Ok(history);
    };
    let allowed = customers.iter().collect::<std::collections::BTreeSet<_>>();
    Ok(db
        .contracts()
        .customer_refs_by_ids(&history, executor)
        .await?
        .into_iter()
        .filter(|row| allowed.contains(&row.customer_id))
        .map(|row| row.id)
        .collect())
}

/// 按请求中的组织筛选收窄已授权客户集合。
///
/// # 参数
/// * `data_scope` - 组织展开与成员事实 Port
/// * `assignments` - 当前主责事实
/// * `context` - 已解析的合同范围事实
/// * `ids` - 当前授权客户集合
/// * `query` - 归一化查询
/// * `as_of` - 客户归属自然日
/// * `executor` - 调用方执行器
///
/// # 返回
/// 无组织筛选时原样返回；否则返回主负责人属于筛选组织的客户。
///
/// # 错误
/// 包含下级但未提供组织、展开失败或成员超限时拒绝。
///
/// # 关键业务约束
/// 组织筛选只看当前主负责人所属组织，不得用协作人员或签约经办组织代替。
async fn apply_org_unit_filter(
    data_scope: &dyn ContractDataScopePort,
    assignments: &dyn crate::ports::CustomerAssignmentFactsPort,
    context: &ContractResolvedScope,
    ids: Option<Vec<String>>,
    query: &ContractListQuery,
    as_of: erp_core::common::time::BusinessDate,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let Some(org_ids) = &query.org_unit_ids else {
        return Ok(ids);
    };
    if query.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded = data_scope
        .expand_org_units(org_ids.as_slice(), query.include_descendants.unwrap_or(false), executor)
        .await?;
    let members = data_scope.org_member_ids(&expanded, context.as_of, executor).await?;
    if members.len() > 10_000 {
        return Err(Error::ValidationError("组织成员超过查询上限，请收窄组织筛选".into()));
    }
    if members.is_empty() {
        return Ok(Some(Vec::new()));
    }
    Ok(Some(assignments.current_owner_customer_ids(ids.as_deref(), Some(&members), as_of, executor).await?))
}

/// 目录范围标签转换为授权集合上的额外收窄。
///
/// # 参数
/// * `scope` - 已证明的合同范围
/// * `requested` - 页面请求的目录范围
///
/// # 返回
/// All 不额外收窄；Assigned 返回当前主责与协作客户。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 目录范围不是授权来源，不能把 All 解释为公司范围。
fn assignment_filter(scope: &ContractReadScope, requested: ContractListScope) -> Option<Vec<String>> {
    match requested {
        ContractListScope::All => None,
        ContractListScope::Assigned => {
            let mut ids = scope.owned_customer_ids.clone();
            ids.extend(scope.collaborative_customer_ids.iter().cloned());
            ids.sort();
            ids.dedup();
            Some(ids)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_pages_without_scope_version_are_rejected() {
        assert!(ensure_page(1, None).is_ok());
        assert!(ensure_page(2, None).is_err());
        assert!(ensure_page(2, Some("")).is_err());
        assert!(ensure_page(2, Some("v1")).is_ok());
        match ensure_page(2, None) {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
    }

    #[test]
    fn mismatched_scope_version_is_data_scope_changed() {
        assert!(ensure_scope_version(None, "v1").is_ok());
        assert!(ensure_scope_version(Some("v1"), "v1").is_ok());
        match ensure_scope_version(Some("v1"), "v2") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        match ensure_stable_snapshot("v1", "v9") {
            Err(Error::ConflictError(message)) => {
                assert!(message.starts_with("DATA_SCOPE_CHANGED："));
            },
            other => panic!("expected DATA_SCOPE_CHANGED, got {other:?}"),
        }
        assert!(ensure_stable_snapshot("v1", "v1").is_ok());
    }

    #[test]
    fn directory_scope_does_not_grant_company_access() {
        let scope = ContractReadScope {
            roles: vec![],
            user_limit: None,
            historical_contract_ids: vec![],
            owned_customer_ids: vec!["c-own".into()],
            collaborative_customer_ids: vec!["c-collab".into()],
            authorized_customer_ids: Some(vec!["c-1".into()]),
        };
        assert!(assignment_filter(&scope, ContractListScope::All).is_none());
        assert_eq!(
            assignment_filter(&scope, ContractListScope::Assigned),
            Some(vec!["c-collab".into(), "c-own".into()])
        );
    }
}
