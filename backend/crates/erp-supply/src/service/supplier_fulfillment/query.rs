use persistence_core::NoTransaction;
use validator::Validate;

use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::{
    FulfillmentOrderListQuery, PageView, SortDir, SupplierFulfillmentOrderListParams,
    SupplierFulfillmentOrderView,
};
use crate::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use crate::repository::prelude::*;
use crate::repository::{FulfillmentOrderReadScope, SupplierFulfillmentExt};
use crate::{Error, Result};

/// 履约订单列表筛选条件类型（经 `SupplierFulfillmentExt` 关联类型跨 crate 可达）。
pub type FulfillmentOrderFilter =
    <mongodb::Database as SupplierFulfillmentExt>::SupplierFulfillmentOrderFilter;

impl SupplierFulfillmentService {
    /// 分页查询供应商履约订单列表（无范围时失败关闭）。
    ///
    /// 授权范围内的列表由组合层注入范围后调用 [`Self::search_fulfillment_orders`]。
    pub async fn supplier_fulfillment_order_list(
        &self,
        params: &SupplierFulfillmentOrderListParams,
    ) -> Result<PageView<SupplierFulfillmentOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let mut filter = fulfillment_order_filter(&query);
        filter.scope = Some(FulfillmentOrderReadScope::default());
        self.search_fulfillment_orders(filter, &mut NoTransaction).await
    }

    /// 按已组装筛选条件分页检索履约订单。
    ///
    /// # 参数
    /// * `filter` - 已含授权与业务筛选的仓储条件
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
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

    /// 按 ID 加载未删除供应商子订单。
    pub async fn load_order(&self, id: &str) -> Result<SupplierFulfillmentOrder> {
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))
    }
}

/// 将规范化查询转换为仓储筛选；授权条件由调用方注入。
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
