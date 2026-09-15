//! 客户列表与候选的一致授权快照；范围与业务版本跨页携带。

use std::hash::{Hash, Hasher};

use application_core::{AuditActor, FilterOption, FilteredPage};
use persistence_core::Transactional;
use serde::Serialize;

use super::CustomerService;
use super::access::intersect_ids;
use crate::dto::customer::{CustomerListParams, CustomerListQuery, CustomerScope, CustomerView};
use crate::error::{Error, Result};
use crate::ports::{CustomerDataScopePort, CustomerResolvedScope};
use crate::repository::scope::CustomerReadScope;
use crate::repository::{CustomerAccountFilter, CustomerExt};

/// 列表响应保持现有字段并声明独立的授权时点及版本。
#[derive(Serialize)]
pub struct CustomerListView {
    /// 分页结果、负责人候选与归属口径。
    #[serde(flatten)]
    pub data: FilteredPage<CustomerView>,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`；有规则但对象为空时不设置。
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
    /// 当前范围内的负责人候选。
    pub owner_options: Vec<FilterOption>,
    /// 身份授权上下文。
    pub context: CustomerResolvedScope,
    /// 授权集合本身为空。
    pub no_scope: bool,
}

impl CustomerService {
    /// 授权、总数、候选与业务身份版本全部在同一个事务读取。
    ///
    /// # 参数
    /// * `params` - 原始查询
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
        _params: &CustomerListParams,
        query: CustomerListQuery,
        actor: &AuditActor,
    ) -> Result<CustomerSnapshot> {
        let db = self.db.clone();
        let access = self.access();
        let data_scope = self.data_scope.clone();
        let party = self.party.clone();
        let accounts = self.accounts.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (mut context, scope) = access.resolve(&actor, "list", executor).await?;
                    let no_scope = !context.has_scope_rules();
                    let authorized =
                        apply_list_filters(&db, data_scope.as_ref(), &context, &scope, &query, executor)
                            .await?;
                    let as_of = super::access::business_date(context.as_of)?;
                    let owners = db
                        .customer_assignments()
                        .current_owners(authorized.as_deref(), None, as_of, executor)
                        .await?;
                    let owner_ids =
                        owners.iter().map(|assignment| assignment.user_id.clone()).collect::<Vec<_>>();
                    let owner_options = accounts.filter_options(&owner_ids).await?;
                    let keyword_party_ids = match query.keyword.as_deref() {
                        Some(keyword) => Some(party.matching_ids_by_name(keyword).await?),
                        None => None,
                    };
                    let filter = CustomerAccountFilter {
                        keyword: query.keyword,
                        keyword_party_ids,
                        party_id: query.party_id,
                        party_ids: None,
                        customer_ids: authorized,
                        status: query.status,
                        page: query.paging.page,
                        page_size: query.paging.page_size,
                        sort_by: Some(query.paging.sort_by.to_string()),
                        sort_ascending: matches!(query.paging.sort_dir, crate::dto::customer::SortDir::Asc),
                    };
                    let page = db.customer_accounts().search_customer_accounts(&filter, executor).await?;
                    let versions = db.customer_accounts().query_versions(&filter, executor).await?;
                    if versions.len() > 10_000 {
                        return Err(Error::ValidationError(
                            "客户查询超过上限，请收窄组织或负责人条件".into(),
                        ));
                    }
                    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
                    versions.hash(&mut fingerprint);
                    context.scope_version = format!("{}:{:x}", context.scope_version, fingerprint.finish());
                    let items = hydrate_rows(
                        &db,
                        (party.as_ref(), accounts.as_ref()),
                        page.items,
                        actor.id(),
                        query.scope,
                        as_of,
                        executor,
                    )
                    .await?;
                    Ok(CustomerSnapshot {
                        no_scope,
                        items,
                        total: page.total,
                        page: filter.page,
                        page_size: filter.page_size,
                        owner_options,
                        context,
                    })
                })
            })
            .await
    }
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
/// 查询过程中范围或客户资料变化时返回 `DATA_SCOPE_CHANGED`。
///
/// # 关键业务约束
/// 复核失败必须整页拒绝，不得返回半新半旧结果。
pub(super) fn ensure_stable_snapshot(first: &str, second: &str) -> Result<()> {
    if first != second {
        return Err(data_scope_changed("数据范围或客户资料已变化，请刷新"));
    }
    Ok(())
}

/// 将业务筛选与授权集合求交。
///
/// # 参数
/// * `db` - 数据库
/// * `data_scope` - 组织展开与成员事实 Port
/// * `context` - 已解析的客户范围事实
/// * `scope` - 已映射的客户范围
/// * `query` - 归一化查询
/// * `executor` - 调用方执行器
///
/// # 返回
/// `None` 表示公司范围且无额外身份筛选；`Some` 为明确客户 ID。
///
/// # 错误
/// 组织展开失败、成员超限或归属查询失败时拒绝。
///
/// # 关键业务约束
/// Mine/协作只是收窄条件；AllAuthorized 不得绕过 DataScope。
async fn apply_list_filters(
    db: &mongodb::Database,
    data_scope: &dyn CustomerDataScopePort,
    context: &CustomerResolvedScope,
    scope: &CustomerReadScope,
    query: &CustomerListQuery,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let as_of = super::access::business_date(context.as_of)?;
    let mut ids = scope.authorized_customer_ids.clone();
    ids = intersect_ids(ids, assignment_filter(scope, query.scope));
    if let Some(owners) = &query.owner_user_ids {
        ids = Some(current_owner_customer_ids(db, ids.as_deref(), owners.as_slice(), as_of, executor).await?);
    }
    apply_org_unit_filter(db, data_scope, context, ids, query, as_of, executor).await
}

/// 按请求中的组织筛选收窄已授权客户集合。
///
/// # 参数
/// * `db` - 数据库
/// * `data_scope` - 组织展开与成员事实 Port
/// * `context` - 已解析的客户范围事实
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
/// 组织筛选只看当前主负责人所属组织，不得用协作人员组织代替。
async fn apply_org_unit_filter(
    db: &mongodb::Database,
    data_scope: &dyn CustomerDataScopePort,
    context: &CustomerResolvedScope,
    ids: Option<Vec<String>>,
    query: &CustomerListQuery,
    as_of: erp_core::common::time::BusinessDate,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<Vec<String>>> {
    let Some(org_ids) = &query.org_unit_ids else {
        if query.include_descendants == Some(true) {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
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
    Ok(Some(current_owner_customer_ids(db, ids.as_deref(), &members, as_of, executor).await?))
}

/// 读取指定负责人集合的当前主责客户。
///
/// # 参数
/// * `db` - 数据库
/// * `customer_ids` - 已授权客户；`None` 表示公司范围
/// * `owners` - 主负责人 ID
/// * `as_of` - 客户归属自然日
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回这些负责人当前主责的客户 ID。
///
/// # 错误
/// 归属查询失败时拒绝。
///
/// # 关键业务约束
/// 空负责人集合保持空结果，不得查询全部客户。
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

/// 将目录范围标签转换为授权集合上的额外收窄。
///
/// # 参数
/// * `scope` - 已证明的客户范围，用于取出当前账号主责与协作客户
/// * `requested` - 页面请求的目录范围
///
/// # 返回
/// AllAuthorized 不额外收窄；其余范围返回对应客户 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 目录范围不是授权来源，不能把 AllAuthorized 解释为公司范围。
fn assignment_filter(scope: &CustomerReadScope, requested: CustomerScope) -> Option<Vec<String>> {
    match requested {
        CustomerScope::AllAuthorized => None,
        CustomerScope::Mine => Some(scope.owned_customer_ids.clone()),
        CustomerScope::Collaborating => Some(scope.collaborative_customer_ids.clone()),
        CustomerScope::Assigned => {
            let mut ids = scope.owned_customer_ids.clone();
            ids.extend(scope.collaborative_customer_ids.iter().cloned());
            ids.sort();
            ids.dedup();
            Some(ids)
        },
    }
}

/// 批量补齐客户当前主体身份与归属。
///
/// # 参数
/// * `db` - 数据库
/// * `facts` - 主体与账号事实端口
/// * `rows` - 当前页客户行
/// * `actor_user_id` - 当前账号
/// * `requested_scope` - 目录范围标签
/// * `as_of` - 归属自然日
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回装配后的列表视图。
///
/// # 错误
/// 跨域读取失败时拒绝整页。
///
/// # 关键业务约束
/// 范围标签只描述命中原因，不授予额外权限。
async fn hydrate_rows(
    db: &mongodb::Database,
    facts: (&dyn crate::ports::PartyFactPort, &dyn crate::ports::AccountFactPort),
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
    let identities = party.identities_by_ids(&party_ids).await?;
    let customer_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let assignments =
        db.customer_assignments().list_active_for_customers(&customer_ids, as_of, executor).await?;
    let account_ids: Vec<String> = assignments.iter().map(|assignment| assignment.user_id.clone()).collect();
    let account_names = accounts.names_by_ids(&account_ids).await?;
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
        let scope = CustomerReadScope {
            roles: vec![],
            user_limit: None,
            historical_customer_ids: vec![],
            owned_customer_ids: vec!["c-own".into()],
            collaborative_customer_ids: vec!["c-collab".into()],
            authorized_customer_ids: Some(vec!["c-1".into()]),
        };
        assert!(assignment_filter(&scope, CustomerScope::AllAuthorized).is_none());
        assert_eq!(assignment_filter(&scope, CustomerScope::Mine), Some(vec!["c-own".into()]));
        assert_eq!(assignment_filter(&scope, CustomerScope::Collaborating), Some(vec!["c-collab".into()]));
    }
}
