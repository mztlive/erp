//! 商品中心聚合读取；查询规则和响应形状由商品领域提供。
use crate::errors::Result;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::service::catalog::{
    prepare_product_list, prepare_sellable_sku_list, product_page_view, sellable_sku_page_view,
};
use erp_catalog::{PageView, ProductListParams, ProductView, SellableSkuListParams, SellableSkuView};
use persistence_core::NoTransaction;
use std::sync::Arc;

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
        let page = self
            .query
            .product_page(&filter, &mut NoTransaction)
            .await
            .map_err(erp_catalog::Error::from)?;
        Ok(product_page_view(page, &filter)?)
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

#[cfg(test)]
mod tests;
