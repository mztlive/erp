//! 为商品页面与销售资格消费方装配同一跨域查询提供方。
use async_trait::async_trait;
use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_catalog::repository::{ProductFilter, ProductRow, SellableSkuFilter, SellableSkuRow};
use erp_core::common::time::BusinessDate;
use erp_read_models::catalog_supply::CatalogSupplyRepository;
use persistence_core::{Executor, PageResult, Result};

use crate::sellable_projection_refresh::SellableProjectionRefresher;

/// 把目录消费方 Port 接到跨域读取，并在列表读取前协调投影追平。
pub struct MongoCatalogSupplyQuery {
    db: mongodb::Database,
}
impl MongoCatalogSupplyQuery {
    /// 绑定数据库；构造不读取事实。
    ///
    /// # 参数
    /// * `db` - 商品与供给集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的查询提供方。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
#[async_trait]
impl CatalogSupplyQueryPort for MongoCatalogSupplyQuery {
    async fn product_candidate_ids(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        CatalogSupplyRepository::new(&self.db).product_candidate_ids(filter, executor).await
    }
    async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>> {
        CatalogSupplyRepository::new(&self.db).product_page(filter, executor).await
    }
    async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>> {
        let query = CatalogSupplyRepository::new(&self.db);
        let restrict = query.sellable_sku_scope(filter, executor).await?;
        if restrict.as_ref().is_some_and(Vec::is_empty) {
            return Ok(PageResult { items: Vec::new(), total: 0 });
        }
        let projection_ready = if filter.eligibility_as_of == BusinessDate::today() {
            SellableProjectionRefresher::new(self.db.clone()).catch_up().await?
        } else {
            false
        };
        query.search_sellable_skus(filter, restrict.as_deref(), projection_ready, executor).await
    }
    async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>> {
        CatalogSupplyRepository::new(&self.db).find_sellable_sku_refs(refs, date, executor).await
    }
    async fn find_sellable_skus_by_ids(
        &self,
        sku_ids: &[String],
        date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>> {
        CatalogSupplyRepository::new(&self.db).find_sellable_skus_by_ids(sku_ids, date, executor).await
    }
}
