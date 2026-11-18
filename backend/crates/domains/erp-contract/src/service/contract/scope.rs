//! 合同列表与范围指标的一致授权快照；范围与业务版本跨页携带。

use application_core::{AuditActor, EmptyScopeVersion, ScopeVersion};
use erp_identity::AccountDirectoryService;
use erp_permissions::contract;
use persistence_core::Executor;
use serde_json::to_vec;

use super::ContractService;
use crate::authorization::ContractAccess;
use crate::dto::contract::{ContractListQuery, ContractListScope, ContractListView, PageView};
use crate::error::{Error, Result};
use crate::ports::ContractResolvedScope;
use crate::repository::list_search::ContractSearch;
use crate::repository::scope::ContractVersion;
use crate::repository::{ContractExt, ContractFilter};

/// 同一事务内的列表快照，供跨页版本复核。
pub(super) struct ContractSnapshot {
    /// 对外列表视图。
    pub view: ContractListView,
    /// 身份授权上下文。
    pub context: ContractResolvedScope,
}

impl ContractService {
    // 授权、总数、范围指标与合同版本全部在同一个事务读取。
    // 返回带范围版本的列表快照。
    // `query` 为已归一化查询；`executor` 为当前列表用例的快照执行器。
    // 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    // 组织筛选按当前主负责人所属组织收窄，不得扩大授权结果。
    pub(super) async fn list_snapshot(
        &self,
        query: &ContractListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ContractSnapshot> {
        let context = self.data_scope.resolve(actor, &contract::LIST, executor).await?;

        let (filter, search, versions) = load_list_filter_and_search(
            &self.db,
            &self.data_scope,
            self.customers.as_ref(),
            &self.accounts,
            &context,
            query,
            executor,
        )
        .await?;

        context.ensure_current(executor)?;
        finish_list_snapshot(&self.db, filter, search, versions, context, executor)
            .await
            .map(|(view, context)| ContractSnapshot { view, context })
    }

    // 只复核列表依赖版本，不重复分页、汇总及展示水合。
    // 同一授权和业务事实口径下的范围版本。
    // query 为同一归一化筛选，actor 为当前账号，executor 为当前列表的快照执行器。
    // 当前授权、筛选事实或仓储读取失败时拒绝。
    pub(super) async fn list_scope_version(
        &self,
        query: &ContractListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let mut context = self.data_scope.resolve(actor, &contract::LIST, executor).await?;
        let (_, search, versions) = load_list_filter_and_search(
            &self.db,
            &self.data_scope,
            self.customers.as_ref(),
            &self.accounts,
            &context,
            query,
            executor,
        )
        .await?;

        context.scope_version = fingerprint_version(&context.scope_version, &versions, &search)?;
        context.validate(executor).await?;
        Ok(context.scope_version)
    }
}

// 求交授权与基础条件并装配列表搜索事实（`list_snapshot` 第二步）。
// 返回基础筛选、结果搜索条件及同一基础范围的有界合同版本集合。
// `db` 为合同数据库；`data_scope` 为组织展开与成员事实 Port。
// `customers` 为客户编号事实；`accounts` 为identity 公开账号目录。
// `context` 为已解析的合同范围事实；`query` 为归一化查询。
// 沿用调用方执行器 `executor`。
// 组织展开失败、基础范围或成员超限、归属查询失败时拒绝。
#[allow(clippy::too_many_arguments)]
async fn load_list_filter_and_search(
    db: &mongodb::Database,
    data_scope: &ContractAccess,
    customers: &dyn crate::ports::CustomerFactsPort,
    accounts: &AccountDirectoryService,
    context: &ContractResolvedScope,
    query: &ContractListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(ContractFilter, ContractSearch, Vec<ContractVersion>)> {
    let customer_ids = query.customer_id.clone().map(|id| vec![id]);
    let mut filter = list_filter(query, customer_ids);
    filter.access = Some(context.clone());
    filter.assigned_user_id =
        (query.scope == ContractListScope::Assigned).then(|| context.user_id().to_owned());
    filter.owner_user_ids = requested_contract_owners(data_scope, context, query, executor).await?;

    let versions = db.contract().query_versions(&filter, executor).await?;
    if versions.len() > 10_000 {
        return Err(Error::ValidationError(
            "合同基础范围超过上限，请收窄客户、组织、合同号或状态条件".into(),
        ));
    }

    let ids = db.contract().list_customer_ids(&filter, executor).await?;
    let customer_facts = super::query::list_customer_facts_with(customers, &ids, executor).await?;
    filter.require_existing_customers(customer_facts.iter().map(|fact| fact.id.clone()).collect());

    let owner_ids = db.contract().list_owner_ids(&filter, executor).await?;
    let owner_names = accounts.names(&owner_ids, executor).await?;

    let search = ContractSearch {
        q: query.q.clone(),
        metric: query.metric,
        settlement_party_id: query.settlement_party_id.clone(),
        owner_user_ids: query.owner_user_ids.clone(),
        customers: customer_facts,
        owners: owner_names,
    };
    Ok((filter, search, versions))
}

// 执行搜索聚合并组装对外视图（`list_snapshot` 第三步）。
// 返回对外视图与携带指纹的授权上下文。
// `db` 为合同数据库；`filter` 为已与授权求交的仓储筛选。
// `search` 为列表搜索条件；`versions` 为同一基础范围已经过规模校验的合同版本集合。
// `context` 为已解析事实（指纹写入范围版本）；沿用调用方执行器 `executor`。
// 聚合或行展示事实读取失败时拒绝。
async fn finish_list_snapshot(
    db: &mongodb::Database,
    filter: ContractFilter,
    search: ContractSearch,
    versions: Vec<ContractVersion>,
    mut context: ContractResolvedScope,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(ContractListView, ContractResolvedScope)> {
    let result = db.contract().search_list(&filter, &search, executor).await?;
    context.scope_version = fingerprint_version(&context.scope_version, &versions, &search)?;
    let total = result.total();
    let items =
        super::query::contract_rows(db, result.items, &search.customers, &search.owners, executor).await?;

    let view = ContractListView {
        ownership_basis: "contract_sales_owner",
        scope_version: String::new(),
        policy_version: 0,
        organization_version: 0,
        as_of: String::new(),
        empty_reason: None,
        scope_summary: "合同负责销售、其所属组织、明确共享及合法单据参与",
        page: PageView { items, total, page: filter.page, page_size: filter.page_size },
        metrics: result.metrics.into_iter().next().unwrap_or_default(),
    };

    context.validate(executor).await?;
    Ok((view, context))
}

// 把授权及列表业务事实绑定到同一个跨页版本。
fn fingerprint_version(
    authorization_version: &str,
    versions: &[ContractVersion],
    search: &ContractSearch,
) -> Result<String> {
    let mut fingerprint = super::fingerprint::version_fingerprint_input(versions);

    let mut customers = search
        .customers
        .iter()
        .map(|customer| (&customer.id, &customer.owner_id, &customer.number, &customer.owner))
        .collect::<Vec<_>>();
    customers.sort_unstable();
    fingerprint.extend(to_vec(&customers).map_err(|error| Error::Internal(error.to_string()))?);
    Ok(format!("{}:{:x}", authorization_version, super::fingerprint::stable_fingerprint(&fingerprint)))
}

// 将内部快照转换为对外列表视图。
// 返回可序列化的列表响应。
// `snapshot` 为同一事务读取的授权与业务快照。
// 不把内部授权证明或全量人员集合返回给客户端。
pub(super) fn to_list_view(snapshot: ContractSnapshot) -> ContractListView {
    let mut view = snapshot.view;
    view.policy_version = snapshot.context.policy_version();
    view.organization_version = snapshot.context.organization_version();
    view.as_of = snapshot.context.as_of().as_utc().to_rfc3339();
    view.scope_version = snapshot.context.scope_version;
    view
}

// 构造可被 HTTP 边界识别的范围变化冲突。
// 返回带 `DATA_SCOPE_CHANGED` 前缀的冲突错误。
// `detail` 为面向用户的中文恢复说明。
// 稳定码必须出现在错误载荷中，供 HTTP 映射为独立 `code`，不得只依赖展示文案。
pub(super) fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

// 后续页必须携带当前范围版本，禁止拼接不同授权快照。
// 第一页或版本非空时成功。
// `page` 为请求页码；`version` 为客户端回传的范围版本。
// 第二页及之后缺少版本时返回 `DATA_SCOPE_CHANGED`。
// 不得在缺版本时继续查询并默默使用新授权。
pub(super) fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    ScopeVersion::new(version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| data_scope_changed(error.detail()))
}

// 客户端回传的范围版本必须与当前快照一致。
// 未携带或完全一致时成功。
// `expected` 为后续页携带的范围版本；第一页可为空；`actual` 为本次查询快照的范围版本。
// 版本不一致时返回 `DATA_SCOPE_CHANGED`。
// 不得把新旧授权结果拼接成同一列表。
pub(super) fn ensure_scope_version(expected: Option<&str>, actual: &str) -> Result<()> {
    ScopeVersion::new(expected, EmptyScopeVersion::Provided)
        .verify(actual)
        .map_err(|error| data_scope_changed(error.detail()))
}

// 同一快照内两次读取的范围版本必须一致。
// 两次版本相同时成功。
// `first` 为首次快照版本；`second` 为复核快照版本。
// 查询过程中范围或合同资料变化时返回 `DATA_SCOPE_CHANGED`。
// 复核失败必须整页拒绝，不得返回半新半旧结果。
pub(super) fn ensure_stable_snapshot(first: &str, second: &str) -> Result<()> {
    ScopeVersion::new(Some(first), EmptyScopeVersion::Provided)
        .verify(second)
        .map_err(|_| data_scope_changed("数据范围或合同资料已变化，请刷新"))
}

// 归一化查询与权限客户集合求交后的仓储条件；空集合必须保留。
// 返回仓储筛选。
// `query` 为归一化查询；`customer_ids` 为请求中的客户筛选。
// 授权凭证由调用方单独绑定，业务筛选不得替代授权。
fn list_filter(query: &ContractListQuery, customer_ids: Option<Vec<String>>) -> ContractFilter {
    ContractFilter {
        access: None,
        contract_no: query.contract_no.clone(),
        customer_id: None,
        customer_ids,
        assigned_user_id: None,
        owner_user_ids: None,
        status: query.status,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, crate::dto::contract::SortDir::Asc),
    }
}
// 请求组织只按合同独立负责销售收窄，不按客户负责名单解释。
async fn requested_contract_owners(
    data_scope: &ContractAccess,
    context: &ContractResolvedScope,
    query: &ContractListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let Some(org_ids) = &query.org_unit_ids else {
        return Ok(None);
    };
    if query.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }

    let organization = data_scope.organization();
    let expanded = organization
        .expand_org_units(org_ids.as_slice(), query.include_descendants.unwrap_or(false), executor)
        .await?;
    let members = organization.org_member_ids(&expanded, context.as_of(), executor).await?;
    if members.len() > 10_000 {
        return Err(Error::ValidationError("组织成员超过查询上限，请收窄组织筛选".into()));
    }
    Ok(Some(members))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::list_search::ContractCustomer;

    #[test]
    fn list_version_tracks_customer_search_facts_and_contract_versions() {
        let versions = [ContractVersion { id: "contract-a".into(), version: 1 }];
        let mut search = ContractSearch {
            customers: vec![ContractCustomer {
                id: "customer-a".into(),
                number: "C-1".into(),
                owner_id: None,
                owner: "—".into(),
            }],
            ..Default::default()
        };
        let version = |versions: &[ContractVersion], search: &ContractSearch| {
            fingerprint_version("authorization-v1", versions, search).unwrap()
        };
        let initial = version(&versions, &search);
        assert_eq!(initial, version(&versions, &search));
        assert_ne!(initial, fingerprint_version("authorization-v2", &versions, &search).unwrap());

        search.customers[0].number = "C-2".into();
        assert_ne!(initial, version(&versions, &search));
        let changed = [ContractVersion { id: "contract-a".into(), version: 2 }];
        assert_ne!(version(&versions, &search), version(&changed, &search));
    }

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
        assert_eq!(
            ensure_scope_version(Some(""), "v1").unwrap_err().to_string(),
            "数据冲突: DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新"
        );
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
}
