//! 销售变更列表与返回前版本复核共用来源授权和筛选。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use application_core::{
    AuditActor, PageView, SortDir, normalize_sort, page_or_default, page_size_or_default,
};
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_sales::dto::sales_review::{SalesChangeOrderListParams, SalesChangeOrderView};
use erp_sales::repository::SalesReviewExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_review::{SalesChangeOrderFilter, SalesChangeOrderRow, SalesChangeVersion};
use persistence_core::{Executor, PageResult, Transactional};

use super::SalesChangeReadService;
use super::query::SalesChangeListView;
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

impl SalesChangeReadService {
    /// 在同一事务读取列表页、总数、来源授权和完整匹配版本。
    ///
    /// # 参数
    /// * `params` - 已通过 HTTP 边界验证的列表参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回原有分页响应及其范围版本。
    ///
    /// # 错误
    /// 权限、筛选、版本规模或仓储读取失败时拒绝。
    pub(super) async fn change_list_snapshot(
        &self,
        params: &SalesChangeOrderListParams,
        actor: &AuditActor,
    ) -> Result<SalesChangeListView> {
        let db = self.db.clone();
        let rbac = self.require_rbac()?.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = SalesAccess::new(db.clone(), rbac);
                    let (mut context, no_scope, filter) =
                        change_list_query(&access, &params, &actor, executor).await?;
                    let page = db.sales_change_orders().search_sales_change_orders(&filter, executor).await?;
                    let versions = db.sales_change_orders().query_change_versions(&filter, executor).await?;
                    context.scope_version = change_scope_version(&context.scope_version, &versions)?;
                    Ok(change_list_view(page, context, no_scope, &filter))
                })
            })
            .await
    }

    /// 在新事务重新证明来源授权，仅读取完整匹配身份与版本。
    ///
    /// # 参数
    /// * `params` - 与首次快照相同的列表参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回与完整快照采用相同授权、筛选和哈希算法的范围版本。
    ///
    /// # 错误
    /// 权限、筛选、版本规模或仓储读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 不复用首次授权来源，也不以当前页或计数代替完整版本集合。
    pub(super) async fn change_scope_fingerprint(
        &self,
        params: &SalesChangeOrderListParams,
        actor: &AuditActor,
    ) -> Result<String> {
        let db = self.db.clone();
        let rbac = self.require_rbac()?.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = SalesAccess::new(db.clone(), rbac);
                    let (context, _, filter) = change_list_query(&access, &params, &actor, executor).await?;
                    let versions = db.sales_change_orders().query_change_versions(&filter, executor).await?;
                    change_scope_version(&context.scope_version, &versions)
                })
            })
            .await
    }
}

/// 每次快照独立解析动作范围与合法来源，筛选只收窄授权结果。
async fn change_list_query(
    access: &SalesAccess,
    params: &SalesChangeOrderListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(AuthorizedDataScope, bool, SalesChangeOrderFilter)> {
    let (context, scope) = access.resolve(actor, "list", &[], executor).await?;
    let no_scope = scope.is_empty();
    let authorized = access.authorized_source_ids(&scope, executor).await?;
    Ok((context, no_scope, change_list_filter(params, authorized)?))
}

/// 列表和版本复核使用相同的来源、状态、分页与排序规范化。
fn change_list_filter(
    params: &SalesChangeOrderListParams,
    authorized: Option<Vec<String>>,
) -> Result<SalesChangeOrderFilter> {
    let (_, sort_dir) = normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
    Ok(SalesChangeOrderFilter {
        sales_order_id: params.sales_order_id.clone(),
        authorized_sales_order_ids: authorized,
        status: params.status,
        page: page_or_default(params.page),
        page_size: page_size_or_default(params.page_size),
        sort_by: Some("created_at".to_string()),
        sort_ascending: matches!(sort_dir, SortDir::Asc),
    })
}

/// 沿用对完整有序版本集合的哈希，超过上限整体拒绝。
fn change_scope_version(base: &str, versions: &[SalesChangeVersion]) -> Result<String> {
    if versions.len() > 10_000 {
        return Err(Error::ValidationError("销售变更查询超过上限，请收窄原销售单条件".into()));
    }
    let mut fingerprint = DefaultHasher::new();
    versions.hash(&mut fingerprint);
    Ok(format!("{base}:{:x}", fingerprint.finish()))
}

/// 只映射首次快照的当前页，保留响应字段、总数及空范围语义。
fn change_list_view(
    page: PageResult<SalesChangeOrderRow>,
    context: AuthorizedDataScope,
    no_scope: bool,
    filter: &SalesChangeOrderFilter,
) -> SalesChangeListView {
    let items = page
        .items
        .into_iter()
        .map(|row| SalesChangeOrderView {
            id: row.id,
            sales_order_id: row.sales_order_id,
            base_revision_id: row.base_revision_id,
            change_type: row.change_type,
            status: row.status,
            current_submission_id: row.current_submission_id,
            version: row.version,
            created_at: row.created_at,
        })
        .collect();
    SalesChangeListView {
        page: PageView { items, total: page.total, page: filter.page, page_size: filter.page_size },
        scope_version: context.scope_version,
        policy_version: context.policy_version,
        organization_version: context.organizations.version,
        as_of: context.as_of.as_utc().to_rfc3339(),
        empty_reason: no_scope.then_some("no_scope"),
        scope_summary: "销售变更单沿来源销售单当前负责人及单据业务组织范围",
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SalesOrderId;
    use erp_sales::entity::sales_review::SalesChangeOrderStatus;
    use persistence_core::QueryFilter;

    use super::*;

    /// 翻页和排序不改变来源、状态及授权交集，空授权仍为空集。
    #[test]
    fn change_filters_keep_authorization_and_business_conditions_across_pages() {
        let mut params = SalesChangeOrderListParams {
            scope_version: None,
            sales_order_id: Some(SalesOrderId::new("so-1")),
            status: Some(SalesChangeOrderStatus::Draft),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let authorized = Some(vec!["so-1".into(), "so-2".into()]);
        let first = change_list_filter(&params, authorized.clone()).unwrap();
        params.page = Some(2);
        params.page_size = Some(50);
        params.sort_dir = Some("asc".into());
        let next = change_list_filter(&params, authorized).unwrap();
        assert_eq!(first.to_doc(), next.to_doc());
        assert_eq!(next.sales_order_id, Some(SalesOrderId::new("so-1")));
        assert_eq!(next.status, Some(SalesChangeOrderStatus::Draft));
        assert_eq!((next.page, next.page_size, next.sort_ascending), (2, 50, true));
        let empty = change_list_filter(&params, Some(Vec::new())).unwrap();
        assert_eq!(empty.authorized_sales_order_ids, Some(Vec::new()));
        assert_ne!(empty.to_doc(), next.to_doc());
        params.sort_by = Some("arbitrary_field".into());
        assert!(matches!(change_list_filter(&params, None), Err(Error::ValidationError(_))));
    }

    /// 指纹保持原完整集合算法，非当前页版本、身份和授权变化均被检测。
    #[test]
    fn change_fingerprint_keeps_full_identity_and_version_binding() {
        let mut versions = vec![
            SalesChangeVersion { id: "change-1".into(), version: 2 },
            SalesChangeVersion { id: "change-2".into(), version: 4 },
        ];
        let mut legacy = DefaultHasher::new();
        versions.hash(&mut legacy);
        let first = change_scope_version("scope", &versions).unwrap();
        assert_eq!(first, format!("scope:{:x}", legacy.finish()));
        versions[1].version += 1;
        assert_ne!(first, change_scope_version("scope", &versions).unwrap());
        versions[1].version -= 1;
        versions[1].id = "change-3".into();
        assert_ne!(first, change_scope_version("scope", &versions).unwrap());
        assert_ne!(first, change_scope_version("new-scope", &versions).unwrap());
        assert_ne!(first, change_scope_version("scope", &[]).unwrap());
    }

    /// 恰好上限保持可查询，超限不得截断成功。
    #[test]
    fn change_fingerprint_rejects_over_limit() {
        let mut versions: Vec<_> = (0..10_000)
            .map(|index| SalesChangeVersion { id: format!("change-{index:05}"), version: 1 })
            .collect();
        assert!(change_scope_version("scope", &versions).is_ok());
        versions.push(SalesChangeVersion { id: "change-over-limit".into(), version: 1 });
        assert!(matches!(
            change_scope_version("scope", &versions),
            Err(Error::ValidationError(message)) if message == "销售变更查询超过上限，请收窄原销售单条件"
        ));
    }
}
