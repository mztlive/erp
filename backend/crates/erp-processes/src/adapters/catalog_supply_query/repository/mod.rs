//! 商品与供给跨域聚合的唯一 Mongo 实现。
mod product;
mod product_pipeline;
mod sellable;
mod shared;

pub(super) struct CatalogSupplyRepository<'a> {
    db: &'a mongodb::Database,
}
impl<'a> CatalogSupplyRepository<'a> {
    pub(super) fn new(db: &'a mongodb::Database) -> Self {
        Self { db }
    }
}
