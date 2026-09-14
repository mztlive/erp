//! 销售变更列表只读取销售集合，不组合审批或财务事实。

use super::{SalesChangeOrderFilter, SalesReviewService};
use crate::dto::sales_review::{self as dto, PageView, SalesChangeOrderListParams, SalesChangeOrderView};
use crate::repository::SalesReviewExt;
use crate::Result;
use erp_core::ids::SalesOrderId;
use persistence_core::NoTransaction;
use validator::Validate;

impl SalesReviewService {
    /// 分页查询销售变更单。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sales_change_order_list(
        &self,
        params: &SalesChangeOrderListParams,
    ) -> Result<PageView<SalesChangeOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesChangeOrderFilter {
            sales_order_id: query.sales_order_id.map(SalesOrderId::new),
            authorized_sales_order_ids: None,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .sales_change_orders()
            .search_sales_change_orders(&filter, &mut NoTransaction)
            .await?;
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

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}
