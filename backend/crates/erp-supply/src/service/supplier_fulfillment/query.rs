use crate::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use crate::repository::SupplierFulfillmentExt;
use persistence_core::NoTransaction;
use validator::Validate;

use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::{
    PageView, SortDir, SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderView,
};
use crate::{Error, Result};

/// 履约订单列表筛选条件类型（经 `SupplierFulfillmentExt` 关联类型跨 crate 可达）。
type FulfillmentOrderFilter = <mongodb::Database as SupplierFulfillmentExt>::SupplierFulfillmentOrderFilter;

impl SupplierFulfillmentService {
    /// 分页查询供应商履约订单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`supplier_id`/三条状态/`external_order_no` 等扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_fulfillment_order_list(
        &self,
        params: &SupplierFulfillmentOrderListParams,
    ) -> Result<PageView<SupplierFulfillmentOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = FulfillmentOrderFilter {
            supplier_id: query.supplier_id,
            fulfillment_status: query.fulfillment_status,
            external_order_no: query.external_order_no,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierFulfillmentOrderView {
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

    /// 按 ID 加载未删除供应商子订单。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回订单实体。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    pub async fn load_order(&self, id: &str) -> Result<SupplierFulfillmentOrder> {
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))
    }
}
