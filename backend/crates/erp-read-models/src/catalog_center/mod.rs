//! 商品中心聚合读取；查询规则和响应形状由商品领域提供。
use std::sync::Arc;

use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::ProductFilter;
use erp_catalog::service::catalog::{
    prepare_product_list, prepare_sellable_sku_list, product_page_view, sellable_sku_page_view,
};
use erp_catalog::{PageView, ProductListParams, ProductView, SellableSkuListParams, SellableSkuView};
use persistence_core::NoTransaction;

use crate::Result;

/// 商品列表与可售商品列表的组合入口。
pub struct CatalogCenterReadService {
    query: Arc<dyn CatalogSupplyQueryPort>,
}
impl CatalogCenterReadService {
    /// 注入同一真实查询提供方；构造不读取事实。
    pub fn new(query: Arc<dyn CatalogSupplyQueryPort>) -> Self {
        Self { query }
    }
    /// 查询商品列表，保持原校验、仓储错误转换与分页投影。
    pub async fn product_list(&self, params: &ProductListParams) -> Result<PageView<ProductView>> {
        let filter = prepare_product_list(params)?;
        let page =
            self.query.product_page(&filter, &mut NoTransaction).await.map_err(erp_catalog::Error::from)?;
        Ok(product_page_view(page, &filter)?)
    }

    /// 按稳定 ID 查询单个商品，与列表共用同一聚合计数口径。
    ///
    /// # 参数
    /// * `id` - 商品稳定 ID
    ///
    /// # 返回
    /// 返回与列表同构的单个商品视图（含真实供给/价格计数）。
    ///
    /// # 错误
    /// 商品不存在时返回 `NotFound("商品不存在")`；仓储失败时透出目录错误。
    pub async fn product_detail(&self, id: &str) -> Result<ProductView> {
        let filter = product_detail_filter(id);
        let page =
            self.query.product_page(&filter, &mut NoTransaction).await.map_err(erp_catalog::Error::from)?;
        let view = product_page_view(page, &filter)?;
        view.items.into_iter().next().ok_or_else(|| crate::Error::NotFound("商品不存在".to_string()))
    }
    /// 查询可售 SKU；资格日期由原商品领域逻辑确定。
    pub async fn sellable_sku_list(
        &self,
        params: &SellableSkuListParams,
    ) -> Result<PageView<SellableSkuView>> {
        let filter = prepare_sellable_sku_list(params)?;
        let page = self
            .query
            .search_sellable_skus(&filter, &mut NoTransaction)
            .await
            .map_err(erp_catalog::Error::from)?;
        Ok(sellable_sku_page_view(page, &filter)?)
    }
}

/// 构造详情单查过滤器，首个 `$match` 即按主键收敛。
fn product_detail_filter(id: &str) -> ProductFilter {
    ProductFilter {
        ids: Some(vec![id.to_string()]),
        product_no: None,
        keyword: None,
        product_kind: None,
        category_id: None,
        brand_id: None,
        supplier_id: None,
        status: None,
        listing_status: None,
        supply_coverage: None,
        sales_price_min: None,
        sales_price_max: None,
        page: 1,
        page_size: 1,
        sort_by: None,
        sort_ascending: false,
    }
}

#[cfg(test)]
mod tests;
