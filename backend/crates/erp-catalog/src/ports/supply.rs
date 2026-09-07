//! 商品消费的跨域列表与精确可售资格查询合同。
use crate::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use persistence_core::{Executor, PageResult, Result};

/// 保持原查询形状、仓储错误与调用方 Executor；实现由组合层注入。
#[async_trait]
pub trait CatalogSupplyQueryPort: Send + Sync {
    /// 分页读取商品和当前 SKU 供给覆盖。
    async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>>;
    /// 分页读取符合原资格条件的可售 SKU。
    async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>>;
    /// 原样复验精确 SKU 与修订引用，不扩大为完整列表查询。
    async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>>;
}
