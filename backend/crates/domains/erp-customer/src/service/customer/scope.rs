//! 客户列表的一致授权快照；范围与业务版本跨页携带。
//! 负责销售候选由销售人员目录提供，不从客户归属记录生成。

use std::sync::Arc;

use application_core::{AuditActor, EmptyScopeVersion, PageView, ScopeVersion};
use erp_core::common::time::BusinessDate;
use erp_identity::AccountDirectoryService;
use erp_permissions::customer as customer_permissions;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use super::CustomerService;
use super::access::ensure_limit;
use crate::dto::customer::{CustomerListQuery, CustomerScope, CustomerView, SortDir};
use crate::error::{Error, Result};
use crate::ports::PartyFactPort;
use crate::repository::scope::{CustomerReadScope, CustomerVersion, authorized_page, authorized_versions};
use crate::repository::{CustomerAccountFilter, CustomerAssignmentRepositoryExt, CustomerExt};
use crate::{CustomerAccess, CustomerResolvedScope};

/// 列表响应保持分页、归属口径与授权时点，不传输负责销售候选。
#[derive(Serialize)]
pub struct CustomerListView {
    /// 分页结果。
    #[serde(flatten)]
    pub data: PageView<CustomerView>,
    /// 当前客户主负责人口径，不代表组织隔离已生效。
    pub ownership_basis: &'static str,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// 授权解析时点。
    pub as_of: String,
    /// 保留响应合同；空结果由完整授权查询产生。
    pub empty_reason: Option<&'static str>,
    /// 当前客户范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
}

/// 同一事务内的列表快照，供跨页版本复核。
pub(super) struct CustomerSnapshot {
    /// 当前页客户。
    pub items: Vec<CustomerView>,
    /// 匹配总数。
    pub total: i64,
    /// 页码。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 身份授权上下文。
    pub context: CustomerResolvedScope,
}

/// 列表事务共用的只读依赖（erp-customer-003）。
///
/// 复核路径与分页读取共用同一过滤与指纹逻辑，仅 `hydrate` 决定是否执行
/// 分页查询与行水合；指纹与总数口径保持一致。
struct ListTxDeps {
    db: Database,
    access: super::access::CustomerAccess,
    party: Arc<dyn PartyFactPort>,
    accounts: AccountDirectoryService,
    actor: AuditActor,
    query: CustomerListQuery,
}

impl CustomerService {
    // 在同一次读取里构建客户目录页，并保留授权条件对应的总数和业务身份版本。
    //
    // # 参数
    // * `query` - 已归一化查询
    // * `executor` - 当前列表用例的快照执行器
    //
    // 返回带范围版本的列表快照，不含负责销售候选。
    //
    // 缺范围版本的后续页、范围变化、筛选非法或仓储失败时拒绝。
    //
    // 组织筛选与负责人筛选只收窄授权客户，不从归属记录生成销售候选。
    pub(super) async fn list_snapshot(
        &self,
        query: CustomerListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerSnapshot> {
        run_list_snapshot(self.list_deps(query, actor), true, executor).await
    }

    // 目录交付前重新计算范围版本，供调用方比对读取期间是否发生变化。
    //
    // 与 [`Self::list_snapshot`] 共用授权、过滤与版本指纹逻辑，
    // 跳过分页查询与行水合；调用方仅用 `context.scope_version`
    // 做稳定性比对，丢弃的 `items` 不再付出水合成本。
    //
    // # 参数
    // * `query` - 已归一化查询
    // * `executor` - 当前列表用例的快照执行器
    //
    // 返回仅带范围版本的轻量快照（`items` 为空、`total` 为零）。
    //
    // 授权或版本读取失败时拒绝，语义与完整快照一致。
    pub(super) async fn list_version_snapshot(
        &self,
        query: CustomerListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerSnapshot> {
        run_list_snapshot(self.list_deps(query, actor), false, executor).await
    }

    // 整理客户列表读取所需的查询、操作人和依赖，供完整查询与轻量版本复核共用。
    //
    // # 参数
    // * `query` - 已归一化查询
    //
    // 返回事务内复用的只读依赖。
    fn list_deps(&self, query: CustomerListQuery, actor: &AuditActor) -> ListTxDeps {
        ListTxDeps {
            db: self.db.clone(),
            access: self.access.clone(),
            party: self.party.clone(),
            accounts: self.accounts.clone(),
            actor: actor.clone(),
            query,
        }
    }
}

// 在同一快照计算列表范围版本，完整读取时同时查询分页行并补全展示字段。
//
// `hydrate` 为假时跳过分页查询与水合，仅重算版本指纹供复核使用。
//
// # 参数
// * `deps` - 列表事务只读依赖
// * `hydrate` - 是否装配分页行
//
// 返回完整或轻量列表快照。
//
// 授权、筛选或版本读取失败时拒绝。
async fn run_list_snapshot(
    deps: ListTxDeps,
    hydrate: bool,
    executor: &mut dyn Executor,
) -> Result<CustomerSnapshot> {
    let (mut context, authorized, as_of) = resolve_list_authorized(&deps, executor).await?;
    let filter = build_account_filter(&deps, authorized, executor).await?;

    let versions = authorized_versions(&deps.db, &filter, &context, &deps.access, executor).await?;
    ensure_limit(versions.len(), "客户查询超过上限，请收窄组织或负责人条件")?;
    apply_version_fingerprint(&mut context, &versions);

    if !hydrate {
        deps.access.validate(&context, executor).await?;
        return Ok(CustomerSnapshot {
            items: Vec::new(),
            total: 0,
            page: filter.page,
            page_size: filter.page_size,
            context,
        });
    }

    let page = authorized_page(&deps.db, &filter, &context, &deps.access, executor).await?;
    let items = hydrate_rows(
        &deps.db,
        (&*deps.party, &deps.accounts),
        page.items,
        deps.actor.id(),
        deps.query.scope,
        as_of,
        executor,
    )
    .await?;

    deps.access.validate(&context, executor).await?;
    Ok(CustomerSnapshot { items, total: page.total, page: filter.page, page_size: filter.page_size, context })
}

// 将操作人可读范围与业务筛选叠加，让目录只返回既匹配条件又可见的客户。
//
// # 参数
// * `deps` - 列表事务只读依赖
//
// 返回授权上下文、授权客户集合与归属时点。
//
// 授权或组织筛选失败时拒绝。
async fn resolve_list_authorized(
    deps: &ListTxDeps,
    executor: &mut dyn Executor,
) -> Result<(CustomerResolvedScope, Option<Vec<String>>, BusinessDate)> {
    let (context, scope) = deps.access.query(&deps.actor, &customer_permissions::LIST, executor).await?;
    let authorized =
        apply_list_filters(&deps.db, &deps.access, &context, &scope, &deps.query, executor).await?;
    let as_of = super::access::business_date(context.as_of)?;
    Ok((context, authorized, as_of))
}

// 把主体名称匹配的客户 ID 和既有可见范围组合为仓储查询条件，供分页及计数使用。
//
// # 参数
// * `deps` - 列表事务只读依赖
// * `authorized` - 已授权客户集合
//
// 返回仓储可消费的账户筛选。
//
// 主体关键词匹配失败时拒绝。
async fn build_account_filter(
    deps: &ListTxDeps,
    authorized: Option<Vec<String>>,
    executor: &mut dyn Executor,
) -> Result<CustomerAccountFilter> {
    let keyword_party_ids = match deps.query.keyword.as_deref() {
        Some(keyword) => Some(deps.party.matching_ids_by_name(keyword, executor).await?),
        None => None,
    };
    Ok(CustomerAccountFilter {
        keyword: deps.query.keyword.clone(),
        keyword_party_ids,
        party_id: deps.query.party_id.clone(),
        party_ids: None,
        customer_ids: authorized,
        status: deps.query.status,
        page: deps.query.paging.page,
        page_size: deps.query.paging.page_size,
        sort_by: Some(deps.query.paging.sort_by.to_string()),
        sort_ascending: matches!(deps.query.paging.sort_dir, SortDir::Asc),
    })
}

// 把客户身份事实的版本指纹合入范围版本，使后续页能检测身份数据变化。
//
// # 参数
// * `context` - 待更新的授权上下文
// * `versions` - 已按 `id` 排序的客户版本
fn apply_version_fingerprint(context: &mut CustomerResolvedScope, versions: &[CustomerVersion]) {
    context.scope_version = fingerprint_versions(
        &context.scope_version,
        versions.iter().map(|version| (version.id.as_str(), version.version)),
    );
}

/// 范围版本指纹的 FNV-1a 64 位封装（erp-customer-004）。
///
/// `DefaultHasher` 跨进程不保证稳定，重启后同授权可能算出不同版本并触发
/// 无谓的 `DATA_SCOPE_CHANGED`。本结构固化字段写入顺序，调用方不得自行
/// 拼接哈希状态。
struct ScopeFingerprint(u64);

impl ScopeFingerprint {
    // 以 FNV-1a 偏移基数创建空指纹。
    fn new() -> Self {
        Self(FNV_OFFSET_BASIS)
    }

    // 写入字符串字节并以 `0xff` 分隔。
    fn feed_str(&mut self, value: &str) {
        for byte in value.as_bytes() {
            self.feed_byte(*byte);
        }
        self.feed_byte(0xff);
    }

    // 写入单个字节。
    fn feed_byte(&mut self, byte: u8) {
        self.0 ^= u64::from(byte);
        self.0 = self.0.wrapping_mul(FNV_PRIME);
    }

    // 写入小端版本字节。
    fn feed_version(&mut self, version: u64) {
        for byte in version.to_le_bytes() {
            self.feed_byte(byte);
        }
    }

    // 拼接到基线版本，返回 `{base}:{hex指纹}`。
    fn finish(self, base: &str) -> String {
        format!("{base}:{:016x}", self.0)
    }
}

// 按稳定顺序计算客户身份版本指纹并合入基线，让列表读前和交付前可以判断范围是否一致。
//
// 与 [`fingerprint_scope_version`] 共用同一 FNV-1a 算法；`query_versions`
// 已按 `id` 排序，调用方按迭代顺序传入 `(id, version)` 即可。
//
// # 参数
// * `base` - 基线范围版本
// * `versions` - 客户身份与版本对
//
// 返回 `{base}:{hex指纹}` 格式的范围版本。
fn fingerprint_versions<'a>(base: &str, versions: impl IntoIterator<Item = (&'a str, u64)>) -> String {
    let mut fingerprint = ScopeFingerprint::new();
    for (id, version) in versions {
        fingerprint.feed_str(id);
        fingerprint.feed_version(version);
    }
    fingerprint.finish(base)
}

/// FNV-1a 64 位偏移基数。
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
/// FNV-1a 64 位素数。
const FNV_PRIME: u64 = 0x100000001b3;

// 将客户目录范围变化表达为稳定业务冲突，供 HTTP 调用方提示重新加载。
//
// # 参数
// * `detail` - 面向用户的中文恢复说明
//
// 返回带 `DATA_SCOPE_CHANGED` 前缀的冲突错误。
//
// 稳定码必须出现在错误载荷中，供 HTTP 映射为独立 `code`，不得只依赖展示文案。
pub(super) fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

// 要求从第二页起带上前页交付的范围版本，以便发现分页期间客户数据或授权范围变化。
//
// # 参数
// * `page` - 请求页码
// * `version` - 客户端回传的范围版本
//
// 第一页或版本非空时成功。
//
// 失败条件：第二页及之后缺少版本时返回 `DATA_SCOPE_CHANGED`。
//
// 不得在缺版本时继续查询并默默使用新授权。
pub(super) fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    ScopeVersion::new(version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| data_scope_changed(error.detail()))
}

// 用当前快照的范围版本核对客户端继续分页的依据，检测上一页读取后发生的变化。
//
// # 参数
// * `expected` - 后续页携带的范围版本；第一页可为空
// * `actual` - 本次查询快照的范围版本
//
// 未携带或完全一致时成功。
//
// 失败条件：版本不一致时返回 `DATA_SCOPE_CHANGED`。
//
// 不得把新旧授权结果拼接成同一列表。
pub(super) fn ensure_scope_version(expected: Option<&str>, actual: &str) -> Result<()> {
    ScopeVersion::new(expected, EmptyScopeVersion::Provided)
        .verify(actual)
        .map_err(|error| data_scope_changed(error.detail()))
}

// 在目录交付前比对两次读取到的范围版本，发现不一致时终止交付。
//
// # 参数
// * `first` - 首次快照版本
// * `second` - 复核快照版本
//
// 两次版本相同时成功。
//
// 失败条件：查询过程中范围或客户资料变化时返回 `DATA_SCOPE_CHANGED`。
//
// 复核失败必须整页拒绝，不得返回半新半旧结果。
pub(super) fn ensure_stable_snapshot(first: &str, second: &str) -> Result<()> {
    ScopeVersion::new(Some(first), EmptyScopeVersion::Provided)
        .verify(second)
        .map_err(|_| data_scope_changed("数据范围或客户资料已变化，请刷新"))
}

// 按人员、组织及请求范围筛选已授权客户，让业务筛选只缩小可见范围。
//
// # 参数
// * `data_scope` - 组织目录访问器
// * `context` - 已解析的客户范围事实
// * `scope` - 已映射的客户范围
// * `query` - 归一化查询
//
// `None` 表示公司范围且无额外身份筛选；`Some` 为明确客户 ID。
//
// 组织展开失败、成员超限或归属查询失败时拒绝。
//
// Mine/协作只是收窄条件；AllAuthorized 不得绕过 DataScope。
async fn apply_list_filters(
    db: &mongodb::Database,
    data_scope: &CustomerAccess,
    context: &CustomerResolvedScope,
    scope: &CustomerReadScope,
    query: &CustomerListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let as_of = super::access::business_date(context.as_of)?;
    let mut ids = assignment_filter(scope, query.scope);
    if let Some(owners) = &query.owner_user_ids {
        ids = Some(current_owner_customer_ids(db, ids.as_deref(), owners.as_slice(), as_of, executor).await?);
    }
    apply_org_unit_filter(db, data_scope, context, ids, query, as_of, executor).await
}

// 按请求中的组织筛选收窄已授权客户集合。
//
// # 参数
// * `data_scope` - 组织目录访问器
// * `context` - 已解析的客户范围事实
// * `ids` - 当前授权客户集合
// * `query` - 归一化查询
// * `as_of` - 客户归属自然日
//
// 无组织筛选时原样返回；否则返回主负责人属于筛选组织的客户。
//
// 包含下级但未提供组织、展开失败或成员超限时拒绝。
//
// 组织筛选只看当前主负责人所属组织，不得用协作人员组织代替。
async fn apply_org_unit_filter(
    db: &mongodb::Database,
    data_scope: &CustomerAccess,
    context: &CustomerResolvedScope,
    ids: Option<Vec<String>>,
    query: &CustomerListQuery,
    as_of: erp_core::common::time::BusinessDate,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let Some(org_ids) = &query.org_unit_ids else {
        if query.include_descendants == Some(true) {
            return Err(Error::ValidationError("请选择一个组织后再勾选包含下级组织".into()));
        }
        return Ok(ids);
    };
    if query.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(Error::ValidationError("请选择一个组织后再勾选包含下级组织".into()));
    }

    let expanded = data_scope
        .organization()
        .expand_org_units(org_ids.as_slice(), query.include_descendants.unwrap_or(false), executor)
        .await?;
    let members = data_scope.organization().org_member_ids(&expanded, context.as_of, executor).await?;
    ensure_limit(members.len(), "组织成员超过查询上限，请收窄组织筛选")?;

    Ok(Some(current_owner_customer_ids(db, ids.as_deref(), &members, as_of, executor).await?))
}

// 按业务日期批量查询指定负责人的有效负责客户，为客户目录的按人筛选提供候选。
//
// # 参数
// * `customer_ids` - 已授权客户；`None` 表示公司范围
// * `owners` - 主负责人 ID
// * `as_of` - 客户归属自然日
//
// 返回这些负责人当前主责的客户 ID。
//
// 归属查询失败时拒绝。
//
// 空负责人集合保持空结果，不得查询全部客户。
async fn current_owner_customer_ids(
    db: &mongodb::Database,
    customer_ids: Option<&[String]>,
    owners: &[String],
    as_of: erp_core::common::time::BusinessDate,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<String>> {
    Ok(db
        .customer_assignments()
        .current_owners(customer_ids, Some(owners), as_of, executor)
        .await?
        .into_iter()
        .map(|assignment| assignment.customer_id.to_string())
        .collect())
}

// 根据目录的范围标签应用对应的归属筛选，所得条件与完整授权范围一起使用。
//
// # 参数
// * `scope` - 已证明的客户范围，用于取出当前账号主责与协作客户
// * `requested` - 页面请求的目录范围
//
// AllAuthorized 不额外收窄；其余范围返回对应客户 ID。
//
// # 关键业务约束
// 目录范围不是授权来源，不能把 AllAuthorized 解释为公司范围。
// `collaborating` 与 `assigned` 按当前主责解释，不再按协作销售展开。
fn assignment_filter(scope: &CustomerReadScope, requested: CustomerScope) -> Option<Vec<String>> {
    match requested {
        CustomerScope::AllAuthorized => None,
        CustomerScope::Mine | CustomerScope::Collaborating | CustomerScope::Assigned => {
            Some(scope.owned_customer_ids.clone())
        },
    }
}

// 将主体身份和归属批量补到已读取的客户列表行，供目录显示主体名称及负责人。
//
// # 参数
// * `facts` - 主体与账号事实端口
// * `rows` - 当前页客户行
// * `actor_user_id` - 当前账号
// * `requested_scope` - 目录范围标签
// * `as_of` - 归属自然日
//
// 返回装配后的列表视图。
//
// 跨域读取失败时拒绝整页。
//
// 范围标签只描述命中原因，不授予额外权限。
async fn hydrate_rows(
    db: &mongodb::Database,
    facts: (&dyn PartyFactPort, &AccountDirectoryService),
    rows: Vec<crate::repository::CustomerAccountRow>,
    actor_user_id: &str,
    requested_scope: CustomerScope,
    as_of: erp_core::common::time::BusinessDate,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<CustomerView>> {
    let (party, accounts) = facts;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let party_ids: Vec<erp_core::ids::PartyId> =
        rows.iter().map(|row| erp_core::ids::PartyId::new(row.party_id.clone())).collect();
    let identities = party.identities_by_ids(&party_ids, executor).await?;

    let customer_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let assignments =
        db.customer_assignments().list_active_for_customers(&customer_ids, as_of, executor).await?;
    let account_ids: Vec<String> = assignments.iter().map(|assignment| assignment.user_id.clone()).collect();
    let account_names = accounts.names(&account_ids, executor).await?;

    Ok(super::assemble_customer_views(
        rows,
        identities,
        assignments,
        actor_user_id,
        requested_scope,
        account_names,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::customer::CustomerListParams;

    #[test]
    fn scope_version_is_consumed_and_org_filter_is_accepted() {
        let query: CustomerListParams = serde_json::from_value(serde_json::json!({
            "page": 2,
            "scope_version": "v1",
            "owner_user_ids": "a,b",
            "org_unit_ids": "org-1,org-2",
            "include_descendants": true
        }))
        .unwrap();
        assert_eq!(query.scope_version.as_deref(), Some("v1"));
        assert_eq!(query.org_unit_ids.unwrap().as_slice(), &["org-1".to_string(), "org-2".to_string()]);
        assert_eq!(query.include_descendants, Some(true));
        assert!(serde_json::from_value::<CustomerListParams>(serde_json::json!({"owner": "张三"})).is_err());
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
        let scope = CustomerReadScope { owned_customer_ids: vec!["c-own".into()] };
        assert!(assignment_filter(&scope, CustomerScope::AllAuthorized).is_none());
        assert_eq!(assignment_filter(&scope, CustomerScope::Mine), Some(vec!["c-own".into()]));
        assert_eq!(assignment_filter(&scope, CustomerScope::Collaborating), Some(vec!["c-own".into()]));
        assert_eq!(assignment_filter(&scope, CustomerScope::Assigned), Some(vec!["c-own".into()]));
    }

    #[test]
    fn fingerprint_is_deterministic_and_sensitive_to_inputs() {
        let versions = [("c-1", 1_u64), ("c-2", 2_u64)];
        let fingerprinted = fingerprint_versions("v1", versions);
        assert!(fingerprinted.starts_with("v1:"));
        assert_ne!(fingerprinted, fingerprint_versions("v1", [("c-1", 1_u64)]));
    }
}
