//! 为商品页面与销售资格消费方装配同一跨域查询提供方。
use async_trait::async_trait;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};
use erp_core::common::time::BusinessDate;
use persistence_core::{Executor, PageResult, Result};
mod repository;
use repository::CatalogSupplyRepository;

/// 商品及可售资格的真实 Mongo 聚合查询提供方。
pub struct MongoCatalogSupplyQuery {
    db: mongodb::Database,
}
impl MongoCatalogSupplyQuery {
    /// 绑定数据库；构造不读取事实。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
#[async_trait]
impl CatalogSupplyQueryPort for MongoCatalogSupplyQuery {
    async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>> {
        CatalogSupplyRepository::new(&self.db)
            .product_page(filter, executor)
            .await
    }
    async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>> {
        CatalogSupplyRepository::new(&self.db)
            .search_sellable_skus(filter, executor)
            .await
    }
    async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>> {
        CatalogSupplyRepository::new(&self.db)
            .find_sellable_sku_refs(refs, date, executor)
            .await
    }
    async fn find_sellable_skus_by_ids(
        &self,
        sku_ids: &[String],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>> {
        CatalogSupplyRepository::new(&self.db)
            .find_sellable_skus_by_ids(sku_ids, date, executor)
            .await
    }
}
