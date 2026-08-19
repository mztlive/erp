//! 域 D10 `catalog` 仓储：product_category、product_brand、unit_of_measure、sku_attribute、
//! sku_attribute_value、product_category_attribute、product(+_revision、_revision_media)、
//! sku(+_revision)、sku_revision_attribute_value、voucher_category_profile_revision。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`super::Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本模块只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一从 `CatalogExt` 关联常量导入。
//!
//! 树形字典 `product_category` 的 P1 实体未定义 `internal_code` 物化路径字段（P1
//! 冻结），子树查询以「层序 `$in` 批量展开」实现：每层一次 `$in` 查询，不产生
//! N+1；`internal_code` 落库后可替换为前缀范围查询（见域报告偏差说明）。
//!
//! 筛选/行类型经 `CatalogExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

mod attribute;
mod category;
mod dictionary;
mod product;
mod product_pipeline;
mod sellable;
mod shared;
mod sku;
mod voucher;

#[allow(unused_imports)]
pub use attribute::{SkuAttributeFilter, SkuAttributeRow, SkuAttributeValueFilter, SkuAttributeValueRow};
#[allow(unused_imports)]
pub use category::{ProductCategoryAttributeFilter, ProductCategoryFilter, ProductCategoryRow};
#[allow(unused_imports)]
pub use dictionary::{ProductBrandFilter, ProductBrandRow, UnitOfMeasureFilter, UnitOfMeasureRow};
#[allow(unused_imports)]
pub use product::{ProductFilter, ProductRevisionFilter, ProductRevisionRow, ProductRow};
#[allow(unused_imports)]
pub use sellable::{SellableSkuFilter, SellableSkuRow};
#[allow(unused_imports)]
pub use sku::{SkuFilter, SkuRevisionFilter, SkuRevisionRow, SkuRow};
#[allow(unused_imports)]
pub use voucher::{VoucherCategoryProfileRevisionFilter, VoucherCategoryProfileRevisionRow};

use mongodb::Database;

/// D10 域专用仓储：跨集合聚合查询与必须位于事务内的多步骤写入。
///
/// 单一集合 CRUD 使用 [`super::Repository`] 基类；本类型承载公司商品池等跨集合
/// 查询，以及依赖事务的跨集合原子写入入口，由 `CatalogExt::catalog()` 访问。
pub struct CatalogRepository<'a> {
    db: &'a Database,
}

impl<'a> CatalogRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}
