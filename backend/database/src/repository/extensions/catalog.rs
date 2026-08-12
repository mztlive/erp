//! 域 D10 `catalog` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as CatalogExt>::PRODUCT_CATEGORIES` 等值。

use entities::catalog::{
    Product, ProductBrand, ProductCategory, ProductCategoryAttribute, ProductRevision, ProductRevisionMedia,
    Sku, SkuAttribute, SkuAttributeValue, SkuRevision, SkuRevisionAttributeValue, UnitOfMeasure,
    VoucherCategoryProfileRevision,
};
use mongodb::Database;

use super::super::catalog::{
    CatalogRepository, ProductBrandFilter, ProductCategoryAttributeFilter, ProductCategoryFilter,
    ProductFilter, ProductRevisionFilter, SellableSkuFilter, SkuAttributeFilter, SkuAttributeValueFilter,
    SkuFilter, SkuRevisionFilter, UnitOfMeasureFilter, VoucherCategoryProfileRevisionFilter,
};
use crate::Repository;

/// 域 D10 仓储访问器。
pub trait CatalogExt {
    /// `product_category` 集合名。
    const PRODUCT_CATEGORIES: &'static str = "product_categories";
    /// `product_brand` 集合名。
    const PRODUCT_BRANDS: &'static str = "product_brands";
    /// `unit_of_measure` 集合名。
    const UNIT_OF_MEASURES: &'static str = "unit_of_measures";
    /// `sku_attribute` 集合名。
    const SKU_ATTRIBUTES: &'static str = "sku_attributes";
    /// `sku_attribute_value` 集合名。
    const SKU_ATTRIBUTE_VALUES: &'static str = "sku_attribute_values";
    /// `product_category_attribute` 集合名。
    const PRODUCT_CATEGORY_ATTRIBUTES: &'static str = "product_category_attributes";
    /// `product` 集合名。
    const PRODUCTS: &'static str = "products";
    /// `product_revision` 集合名。
    const PRODUCT_REVISIONS: &'static str = "product_revisions";
    /// `product_revision_media` 集合名。
    const PRODUCT_REVISION_MEDIAS: &'static str = "product_revision_medias";
    /// `sku` 集合名。
    const SKUS: &'static str = "skus";
    /// `sku_revision` 集合名。
    const SKU_REVISIONS: &'static str = "sku_revisions";
    /// `sku_revision_attribute_value` 集合名。
    const SKU_REVISION_ATTRIBUTE_VALUES: &'static str = "sku_revision_attribute_values";
    /// `voucher_category_profile_revision` 集合名。
    const VOUCHER_CATEGORY_PROFILE_REVISIONS: &'static str = "voucher_category_profile_revisions";

    /// 商品分类列表筛选条件类型（定义见 `repository::catalog`）。
    type ProductCategoryFilter;

    /// 商品品牌列表筛选条件类型（定义见 `repository::catalog`）。
    type ProductBrandFilter;

    /// 计量单位列表筛选条件类型（定义见 `repository::catalog`）。
    type UnitOfMeasureFilter;

    /// 规格属性列表筛选条件类型（定义见 `repository::catalog`）。
    type SkuAttributeFilter;

    /// 规格属性值列表筛选条件类型（定义见 `repository::catalog`）。
    type SkuAttributeValueFilter;

    /// 分类-属性适用关系列表筛选条件类型（定义见 `repository::catalog`）。
    type ProductCategoryAttributeFilter;

    /// 商品列表筛选条件类型（定义见 `repository::catalog`）。
    type ProductFilter;

    /// 公司商品池列表筛选条件类型（定义见 `repository::catalog`）。
    type SellableSkuFilter;

    /// 商品修订列表筛选条件类型（定义见 `repository::catalog`）。
    type ProductRevisionFilter;

    /// SKU 列表筛选条件类型（定义见 `repository::catalog`）。
    type SkuFilter;

    /// SKU 修订列表筛选条件类型（定义见 `repository::catalog`）。
    type SkuRevisionFilter;

    /// 卡券类目扩展修订列表筛选条件类型（定义见 `repository::catalog`）。
    type VoucherCategoryProfileRevisionFilter;

    /// 获取 `product_category` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductCategory>`。
    fn product_categories(&self) -> Repository<'_, ProductCategory>;

    /// 获取 `product_brand` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductBrand>`。
    fn product_brands(&self) -> Repository<'_, ProductBrand>;

    /// 获取 `unit_of_measure` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::UnitOfMeasure>`。
    fn unit_of_measures(&self) -> Repository<'_, UnitOfMeasure>;

    /// 获取 `sku_attribute` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuAttribute>`。
    fn sku_attributes(&self) -> Repository<'_, SkuAttribute>;

    /// 获取 `sku_attribute_value` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuAttributeValue>`。
    fn sku_attribute_values(&self) -> Repository<'_, SkuAttributeValue>;

    /// 获取 `product_category_attribute` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductCategoryAttribute>`。
    fn product_category_attributes(&self) -> Repository<'_, ProductCategoryAttribute>;

    /// 获取 `product` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::Product>`。
    fn products(&self) -> Repository<'_, Product>;

    /// 获取 `product_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductRevision>`。
    fn product_revisions(&self) -> Repository<'_, ProductRevision>;

    /// 获取 `product_revision_media` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductRevisionMedia>`。
    fn product_revision_medias(&self) -> Repository<'_, ProductRevisionMedia>;

    /// 获取 `sku` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::Sku>`。
    fn skus(&self) -> Repository<'_, Sku>;

    /// 获取 `sku_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuRevision>`。
    fn sku_revisions(&self) -> Repository<'_, SkuRevision>;

    /// 获取 `sku_revision_attribute_value` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuRevisionAttributeValue>`。
    fn sku_revision_attribute_values(&self) -> Repository<'_, SkuRevisionAttributeValue>;

    /// 获取 `voucher_category_profile_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::VoucherCategoryProfileRevision>`。
    fn voucher_category_profile_revisions(&self) -> Repository<'_, VoucherCategoryProfileRevision>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `CatalogRepository` 实例。
    fn catalog(&self) -> CatalogRepository<'_>;
}

impl CatalogExt for Database {
    type ProductCategoryFilter = ProductCategoryFilter;
    type ProductBrandFilter = ProductBrandFilter;
    type UnitOfMeasureFilter = UnitOfMeasureFilter;
    type SkuAttributeFilter = SkuAttributeFilter;
    type SkuAttributeValueFilter = SkuAttributeValueFilter;
    type ProductCategoryAttributeFilter = ProductCategoryAttributeFilter;
    type ProductFilter = ProductFilter;
    type SellableSkuFilter = SellableSkuFilter;
    type ProductRevisionFilter = ProductRevisionFilter;
    type SkuFilter = SkuFilter;
    type SkuRevisionFilter = SkuRevisionFilter;
    type VoucherCategoryProfileRevisionFilter = VoucherCategoryProfileRevisionFilter;

    /// 获取 `product_category` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductCategory>`。
    fn product_categories(&self) -> Repository<'_, ProductCategory> {
        Repository::new(self, Self::PRODUCT_CATEGORIES)
    }

    /// 获取 `product_brand` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductBrand>`。
    fn product_brands(&self) -> Repository<'_, ProductBrand> {
        Repository::new(self, Self::PRODUCT_BRANDS)
    }

    /// 获取 `unit_of_measure` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::UnitOfMeasure>`。
    fn unit_of_measures(&self) -> Repository<'_, UnitOfMeasure> {
        Repository::new(self, Self::UNIT_OF_MEASURES)
    }

    /// 获取 `sku_attribute` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuAttribute>`。
    fn sku_attributes(&self) -> Repository<'_, SkuAttribute> {
        Repository::new(self, Self::SKU_ATTRIBUTES)
    }

    /// 获取 `sku_attribute_value` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuAttributeValue>`。
    fn sku_attribute_values(&self) -> Repository<'_, SkuAttributeValue> {
        Repository::new(self, Self::SKU_ATTRIBUTE_VALUES)
    }

    /// 获取 `product_category_attribute` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductCategoryAttribute>`。
    fn product_category_attributes(&self) -> Repository<'_, ProductCategoryAttribute> {
        Repository::new(self, Self::PRODUCT_CATEGORY_ATTRIBUTES)
    }

    /// 获取 `product` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::Product>`。
    fn products(&self) -> Repository<'_, Product> {
        Repository::new(self, Self::PRODUCTS)
    }

    /// 获取 `product_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductRevision>`。
    fn product_revisions(&self) -> Repository<'_, ProductRevision> {
        Repository::new(self, Self::PRODUCT_REVISIONS)
    }

    /// 获取 `product_revision_media` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::ProductRevisionMedia>`。
    fn product_revision_medias(&self) -> Repository<'_, ProductRevisionMedia> {
        Repository::new(self, Self::PRODUCT_REVISION_MEDIAS)
    }

    /// 获取 `sku` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::Sku>`。
    fn skus(&self) -> Repository<'_, Sku> {
        Repository::new(self, Self::SKUS)
    }

    /// 获取 `sku_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuRevision>`。
    fn sku_revisions(&self) -> Repository<'_, SkuRevision> {
        Repository::new(self, Self::SKU_REVISIONS)
    }

    /// 获取 `sku_revision_attribute_value` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::SkuRevisionAttributeValue>`。
    fn sku_revision_attribute_values(&self) -> Repository<'_, SkuRevisionAttributeValue> {
        Repository::new(self, Self::SKU_REVISION_ATTRIBUTE_VALUES)
    }

    /// 获取 `voucher_category_profile_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::catalog::VoucherCategoryProfileRevision>`。
    fn voucher_category_profile_revisions(&self) -> Repository<'_, VoucherCategoryProfileRevision> {
        Repository::new(self, Self::VOUCHER_CATEGORY_PROFILE_REVISIONS)
    }

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `CatalogRepository` 实例。
    fn catalog(&self) -> CatalogRepository<'_> {
        CatalogRepository::new(self)
    }
}
