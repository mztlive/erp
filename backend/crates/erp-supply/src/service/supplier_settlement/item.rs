use crate::repository::SupplierSettlementExt;
use persistence_core::NoTransaction;
use validator::Validate;

use super::dto::{SettlementPageView, SupplierSettlementItemListParams, SupplierSettlementItemView};
use super::SupplierSettlementService;
use crate::dto::supplier_fulfillment::SortDir;
use crate::Result;

/// 结算明细列表筛选条件类型。
type ItemFilter = <mongodb::Database as SupplierSettlementExt>::SupplierSettlementItemFilter;

impl SupplierSettlementService {
    /// 分页查询供应商结算明细列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_item_list(
        &self,
        params: &SupplierSettlementItemListParams,
    ) -> Result<SettlementPageView<SupplierSettlementItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ItemFilter {
            statement_id: query.statement_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_settlement_items()
            .search_supplier_settlement_items(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementItemView {
                id: row.id,
                statement_id: row.statement_id.to_string(),
                supplier_fulfillment_order_id: row.supplier_fulfillment_order_id.to_string(),
                supplier_fulfillment_item_id: row.supplier_fulfillment_item_id.to_string(),
                quantity: row.quantity,
                order_amount: row.order_amount,
                freight_amount: row.freight_amount,
                service_fee_amount: row.service_fee_amount,
                refund_amount: row.refund_amount,
                erp_calculated_amount: row.erp_calculated_amount,
                erp_calculated_net_amount: row.erp_calculated_net_amount,
                erp_calculated_tax_amount: row.erp_calculated_tax_amount,
                supplier_billed_amount: row.supplier_billed_amount,
                supplier_billed_net_amount: row.supplier_billed_net_amount,
                supplier_billed_tax_amount: row.supplier_billed_tax_amount,
                created_at: row.created_at,
            })
            .collect();

        Ok(SettlementPageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}
