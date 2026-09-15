//! 原商品列表聚合及同一 Executor 的 Mongo 执行。
use erp_catalog::entity::catalog::Product;
use erp_catalog::repository::{CatalogExt, ProductFilter, ProductRow};
use futures_util::TryStreamExt;
use mongodb::bson::Document;
use persistence_core::{Executor, PageResult, Result};
use serde::Deserialize;

use super::CatalogSupplyRepository;
use super::product_pipeline::product_list_pipeline;

/// 商品列表聚合分页结果。
#[derive(Debug, Deserialize)]
struct ProductFacet {
    /// 当前页数据。
    items: Vec<ProductRow>,
    /// 总数聚合行。
    total: Vec<ProductTotal>,
}

/// 商品列表总数聚合行。
#[derive(Debug, Deserialize)]
struct ProductTotal {
    /// 符合筛选的商品数量。
    count: i64,
}

impl CatalogSupplyRepository<'_> {
    /// 分页查询商品及当前启用 SKU 的聚合筛选结果。
    ///
    /// 统一关键字覆盖商品编号/名称与 SKU 编号/名称/规格/条码；上架状态、
    /// 供给覆盖和销售价区间均按当前启用 SKU 实时派生，不在商品主表冗余落库。
    ///
    /// # 参数
    /// * `filter` - 商品、当前修订与 SKU 聚合筛选条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页商品投影与满足筛选条件的总数。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn search_products(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>> {
        let facet = self.aggregate_products(product_list_pipeline(filter), executor).await?;
        Ok(PageResult { items: facet.items, total: facet.total.first().map_or(0, |row| row.count) })
    }

    /// 按语义化筛选条件分页查询商品聚合结果。
    ///
    /// # 参数
    /// * `filter` - 商品、当前 SKU 关系、价格、分页与排序条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回商品聚合投影分页结果。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn product_page(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>> {
        self.search_products(filter, executor).await
    }

    /// 执行商品列表类型化聚合并收集唯一的 facet 结果。
    async fn aggregate_products(
        &self,
        pipeline: Vec<Document>,
        executor: &mut dyn Executor,
    ) -> Result<ProductFacet> {
        let collection = self.db.collection::<Product>(<mongodb::Database as CatalogExt>::PRODUCTS);
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<ProductFacet>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            },
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<ProductFacet>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            },
        };
        Ok(rows.into_iter().next().unwrap_or(ProductFacet { items: Vec::new(), total: Vec::new() }))
    }
}
