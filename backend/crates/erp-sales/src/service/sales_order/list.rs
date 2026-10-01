//! 销售列表与版本重验共用同一规范化筛选。

use application_core::PageView;
use persistence_core::Executor;
use validator::Validate;

use super::SalesOrderService;
use crate::dto::sales_order::{SalesOrderListParams, SortDir};
use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
use crate::repository::sales_order::scope::{SalesReadScope, SalesVersion};
use crate::repository::sales_order::{SalesOrderFilter, SalesOrderListView, SalesOrderRow, SalesOrderSearch};
use crate::{Error, Result};

impl SalesOrderService {
    /// 在调用方授权快照中读取列表页和全部匹配版本。
    ///
    /// # 参数
    /// * `params` - 原始列表参数
    /// * `search` - 已解析的关键词关联事实
    /// * `scope` - 已证明的销售授权范围
    /// * `business_org_unit_ids` - 已展开的组织筛选
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 当前页、总数及同口径的版本集合。
    ///
    /// # 错误
    /// 参数非法、匹配集合超过一万条或仓储失败时拒绝。
    pub async fn list_rows(
        &self,
        params: &SalesOrderListParams,
        search: SalesOrderSearch,
        scope: &SalesReadScope,
        business_org_unit_ids: Option<Vec<String>>,
        executor: &mut dyn Executor,
    ) -> Result<(PageView<SalesOrderRow>, Vec<SalesVersion>)> {
        let filter = list_filter(params, search, business_org_unit_ids)?;
        let page = self.db.sales_orders().search_sales_orders(&filter, scope, executor).await?;
        let versions = self.filtered_versions(&filter, scope, executor).await?;
        Ok((
            PageView { items: page.items, total: page.total, page: filter.page, page_size: filter.page_size },
            versions,
        ))
    }

    /// 仅重验版本集合，不重复加载列表行和总数。
    ///
    /// # 参数
    /// * `params` - 与首次读取相同的列表参数
    /// * `search` - 与首次读取相同的关键词关联事实
    /// * `scope` - 本次重新证明的销售授权范围
    /// * `business_org_unit_ids` - 本次重新展开的组织筛选
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 与列表查询同口径、按身份稳定排序的版本集合。
    ///
    /// # 错误
    /// 参数非法、匹配集合超过一万条或仓储失败时拒绝。
    pub async fn list_versions(
        &self,
        params: &SalesOrderListParams,
        search: SalesOrderSearch,
        scope: &SalesReadScope,
        business_org_unit_ids: Option<Vec<String>>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesVersion>> {
        let filter = list_filter(params, search, business_org_unit_ids)?;
        self.filtered_versions(&filter, scope, executor).await
    }

    /// 从已授权的筛选读取版本并执行与列表相同的规模上限。
    async fn filtered_versions(
        &self,
        filter: &SalesOrderFilter,
        scope: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesVersion>> {
        let versions = self.db.sales_orders().query_versions(filter, scope, executor).await?;
        if versions.len() > 10_000 {
            return Err(Error::ValidationError("销售单查询超过上限，请收窄期间或客户条件".into()));
        }
        Ok(versions)
    }
}

/// 复用领域 DTO 规范化，统一构造列表和版本重验的全部业务筛选。
fn list_filter(
    params: &SalesOrderListParams,
    search: SalesOrderSearch,
    business_org_unit_ids: Option<Vec<String>>,
) -> Result<SalesOrderFilter> {
    params.validate()?;
    let query = params.normalized()?;
    Ok(SalesOrderFilter {
        search,
        order_no: query.order_no,
        customer_id: query.customer_id,
        contract_id: query.contract_id,
        origin_system: query.origin_system,
        commercial_status: query.commercial_status,
        review_status: query.review_status,
        business_type: query.business_type,
        fulfillment_progress: query.fulfillment_progress,
        collection_progress: query.collection_progress,
        invoice_progress: query.invoice_progress,
        close_status: query.close_status,
        created_from: query.created_from,
        created_to: query.created_to,
        created_by: query.created_by,
        owner_user_ids: query.owner_user_ids,
        business_org_unit_ids,
        view: SalesOrderListView::from_flags(query.my_todo, query.exception_only)?,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    })
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;
    use serde_json::json;

    use super::*;

    /// 翻页和排序不得改变版本集合；责任人、组织、期间和关键词仍保持相同筛选。
    #[test]
    fn paging_changes_preserve_the_normalized_business_filter() {
        let mut params: SalesOrderListParams = serde_json::from_value(json!({
            "order_no": " SO-1 ", "created_by": " creator ", "owner_user_ids": "sales-1,sales-2",
            "org_unit_ids": "org-1", "include_descendants": true,
            "created_from": 10, "created_to": 20, "customer_id": "customer-1", "contract_id": "contract-1"
        }))
        .unwrap();
        let search = SalesOrderSearch { q: Some("客户".into()), ..Default::default() };
        let orgs = Some(vec!["org-1".into(), "org-child".into()]);
        let first = list_filter(&params, search.clone(), orgs.clone()).unwrap();
        params.page = Some(2);
        params.page_size = Some(50);
        params.sort_by = Some("order_no".into());
        params.sort_dir = Some("asc".into());
        let next = list_filter(&params, search, orgs.clone()).unwrap();
        assert_eq!(first.to_doc(), next.to_doc());
        assert_eq!(next.page, 2);
        assert_eq!(next.page_size, 50);
        assert_eq!(next.order_no.as_deref(), Some("SO-1"));
        assert_eq!(next.created_by.as_deref(), Some("creator"));
        assert_eq!(next.business_org_unit_ids, orgs);
        assert_eq!(next.owner_user_ids.unwrap().as_slice(), &["sales-1", "sales-2"]);
        assert_eq!((next.created_from, next.created_to), (Some(10), Some(20)));
        assert_eq!(next.customer_id.as_deref(), Some("customer-1"));
        assert_eq!(next.contract_id.as_deref(), Some("contract-1"));
        assert_eq!(next.search.q.as_deref(), Some("客户"));
    }

    /// 共享筛选工厂保留排序白名单、期间、组织和互斥视图的拒绝规则。
    #[test]
    fn invalid_filters_are_rejected_before_querying_versions() {
        for value in [
            json!({"sort_by": "arbitrary_field"}),
            json!({"sort_dir": "invalid"}),
            json!({"created_from": 20, "created_to": 10}),
            json!({"include_descendants": true}),
            json!({"my_todo": true, "exception_only": true}),
        ] {
            let params = serde_json::from_value(value).unwrap();
            assert!(matches!(
                list_filter(&params, SalesOrderSearch::default(), None),
                Err(Error::ValidationError(_))
            ));
        }
    }
}
