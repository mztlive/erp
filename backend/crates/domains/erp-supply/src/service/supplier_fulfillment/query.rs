use super::SupplierFulfillmentService;
use crate::Result;
use crate::dto::supplier_fulfillment::{
    FulfillmentOrderListQuery, PageView, SortDir, SupplierFulfillmentOrderView,
};
use crate::repository::SupplierFulfillmentExt;
use crate::repository::prelude::*;
pub use crate::repository::supplier_fulfillment::SupplierFulfillmentOrderFilter as FulfillmentOrderFilter;

impl SupplierFulfillmentService {
    /// 用已含授权范围和业务筛选的条件读履约订单页与总数，再映射为列表视图；沿用调用方执行器，仓储错误不
    /// 转换为空页。
    pub async fn search_fulfillment_orders(
        &self,
        filter: FulfillmentOrderFilter,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<PageView<SupplierFulfillmentOrderView>> {
        let page = self
            .db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&filter, executor)
            .await?;
        let items = page.items.into_iter().map(row_view).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }
}

/// 把规范化的订单、状态、责任人和分页参数转换为仓储筛选。这里不授予访问范围；调用方须注入已编译授权后才
/// 可取数。
pub fn fulfillment_order_filter(query: &FulfillmentOrderListQuery) -> FulfillmentOrderFilter {
    FulfillmentOrderFilter {
        q: query.q.clone(),
        supplier_id: query.supplier_id.clone(),
        fulfillment_status: query.fulfillment_status,
        cancel_status: query.cancel_status,
        refund_status: query.refund_status,
        view: query.view.clone(),
        aftersale_pending: query.aftersale_pending,
        scope: None,
        follow_up_user_ids: query.owner_user_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        business_org_unit_ids: query.org_unit_ids.as_ref().map(|ids| ids.as_slice().to_vec()),
        handler_order_ids: None,
        external_order_no: query.external_order_no.clone(),
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

fn row_view(
    row: crate::repository::supplier_fulfillment::SupplierFulfillmentOrderRow,
) -> SupplierFulfillmentOrderView {
    SupplierFulfillmentOrderView {
        id: row.id,
        fulfillment_order_no: row.fulfillment_order_no,
        supplier_id: row.supplier_id.to_string(),
        connection_id: row.connection_id.to_string(),
        split_no: row.split_no,
        fulfillment_status: row.fulfillment_status,
        cancel_status: row.cancel_status,
        refund_status: row.refund_status,
        external_order_no: row.external_order_no,
        submitted_at: row.submitted_at.map(|t| t.unix_secs()),
        accepted_at: row.accepted_at.map(|t| t.unix_secs()),
        completed_at: row.completed_at.map(|t| t.unix_secs()),
        follow_up_user_id: row.follow_up_user_id,
        business_org_unit_id: row.business_org_unit_id,
        handler_user_id: None,
        version: row.version,
        created_at: row.created_at,
    }
}
