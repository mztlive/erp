//! 商品与供给跨域聚合的唯一 Mongo 实现。
mod product;
mod product_pipeline;
mod sellable;
mod shared;

pub(super) struct CatalogSupplyRepository<'a> {
    db: &'a mongodb::Database,
}
impl<'a> CatalogSupplyRepository<'a> {
    /// 借用调用方数据库；构造不打开连接或事务。
    ///
    /// # 参数
    /// * `db` - 商品与供给集合所在数据库。
    ///
    /// # 返回
    /// 返回绑定该借用的聚合仓库。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn new(db: &'a mongodb::Database) -> Self {
        Self { db }
    }
}
