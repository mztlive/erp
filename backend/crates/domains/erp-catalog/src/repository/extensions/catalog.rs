//! 域 D10 `catalog` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as CatalogExt>::PRODUCT_CATEGORIES` 等值。

use mongodb::Database;

use crate::entity::catalog::SkuRevisionAttributeValue;
use crate::repository::catalog::{CatalogRepository, SellableSkuProjectionStore};
use crate::repository::owned::{
    ProductBrandRepository, ProductCategoryAttributeRepository, ProductCategoryRepository, ProductRepository,
    ProductRevisionMediaRepository, ProductRevisionRepository, SkuAttributeRepository,
    SkuAttributeValueRepository, SkuBarcodeClaimRepository, SkuRepository, SkuRevisionRepository,
    UnitOfMeasureRepository, VoucherCategoryProfileRevisionRepository,
};

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
    /// `sku_barcode_claims` 以实际条码保存唯一 SKU 归属。
    const SKU_BARCODE_CLAIMS: &'static str = "sku_barcode_claims";
    /// `sku_revision_attribute_value` 集合名。
    const SKU_REVISION_ATTRIBUTE_VALUES: &'static str = "sku_revision_attribute_values";
    /// `voucher_category_profile_revision` 集合名。
    const VOUCHER_CATEGORY_PROFILE_REVISIONS: &'static str = "voucher_category_profile_revisions";
    /// 公司商品池预算投影集合名。
    const SELLABLE_SKU_PROJECTIONS: &'static str = "sellable_sku_projections";
    /// 公司商品池预算投影刷新进度集合名。
    const SELLABLE_SKU_PROJECTION_STATE: &'static str = "sellable_sku_projection_state";

    /// 绑定 `product_category` 集合仓储，供领域查询或写入步骤复用。返回 `ProductCategoryRepository<'_>`。
    fn product_categories(&self) -> ProductCategoryRepository<'_>;

    /// 绑定 `product_brand` 集合仓储，供领域查询或写入步骤复用。返回 `ProductBrandRepository<'_>`。
    fn product_brands(&self) -> ProductBrandRepository<'_>;

    /// 绑定 `unit_of_measure` 集合仓储，供领域查询或写入步骤复用。返回 `UnitOfMeasureRepository<'_>`。
    fn unit_of_measures(&self) -> UnitOfMeasureRepository<'_>;

    /// 绑定 `sku_attribute` 集合仓储，供领域查询或写入步骤复用。返回 `SkuAttributeRepository<'_>`。
    fn sku_attributes(&self) -> SkuAttributeRepository<'_>;

    /// 绑定 `sku_attribute_value` 集合仓储，供领域查询或写入步骤复用。返回 `SkuAttributeValueRepository<'_>`。
    fn sku_attribute_values(&self) -> SkuAttributeValueRepository<'_>;

    /// 绑定 `product_category_attribute` 集合仓储，供领域查询或写入步骤复用。
    ///
    /// 返回 `ProductCategoryAttributeRepository<'_>`。
    fn product_category_attributes(&self) -> ProductCategoryAttributeRepository<'_>;

    /// 绑定 `product` 集合仓储，供领域查询或写入步骤复用。返回 `ProductRepository<'_>`。
    fn products(&self) -> ProductRepository<'_>;

    /// 绑定 `product_revision` 集合仓储，供领域查询或写入步骤复用。返回 `ProductRevisionRepository<'_>`。
    fn product_revisions(&self) -> ProductRevisionRepository<'_>;

    /// 绑定 `product_revision_media` 集合仓储，供领域查询或写入步骤复用。返回
    /// `ProductRevisionMediaRepository<'_>`。
    fn product_revision_medias(&self) -> ProductRevisionMediaRepository<'_>;

    /// 绑定 `sku` 集合仓储，供领域查询或写入步骤复用。返回 `SkuRepository<'_>`。
    fn skus(&self) -> SkuRepository<'_>;

    /// 绑定 `sku_revision` 集合仓储，供领域查询或写入步骤复用。返回 `SkuRevisionRepository<'_>`。
    fn sku_revisions(&self) -> SkuRevisionRepository<'_>;

    /// 获取实际条码唯一归属占用仓储。当前数据库的标准乐观锁仓储。
    fn sku_barcode_claims(&self) -> SkuBarcodeClaimRepository<'_>;

    /// 绑定 `sku_revision_attribute_value` 集合仓储，供领域查询或写入步骤复用。
    ///
    /// 返回 `persistence_core::Repository<'_, crate::entity::catalog::SkuRevisionAttributeValue>`。
    fn sku_revision_attribute_values(&self) -> persistence_core::Repository<'_, SkuRevisionAttributeValue>;

    /// 绑定 `voucher_category_profile_revision` 集合仓储，供领域查询或写入步骤复用。
    ///
    /// 返回 `VoucherCategoryProfileRevisionRepository<'_>`。
    fn voucher_category_profile_revisions(&self) -> VoucherCategoryProfileRevisionRepository<'_>;

    /// 绑定本域跨集合用例的仓储；读写和事务边界仍由调用方传入的执行器决定。返回 `CatalogRepository` 实例。
    fn catalog(&self) -> CatalogRepository<'_>;

    /// 获取公司商品池预算投影访问器。
    fn sellable_sku_projections(&self) -> SellableSkuProjectionStore<'_>;
}

impl CatalogExt for Database {
    // 绑定 `product_category` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `PRODUCT_CATEGORIES` 构造，不在此访问集合。
    //
    // 返回 `ProductCategoryRepository<'_>`。
    fn product_categories(&self) -> ProductCategoryRepository<'_> {
        ProductCategoryRepository::new(self, Self::PRODUCT_CATEGORIES)
    }

    // 绑定 `product_brand` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `PRODUCT_BRANDS` 构造，不在此访问集合。
    //
    // 返回 `ProductBrandRepository<'_>`。
    fn product_brands(&self) -> ProductBrandRepository<'_> {
        ProductBrandRepository::new(self, Self::PRODUCT_BRANDS)
    }

    // 绑定 `unit_of_measure` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `UNIT_OF_MEASURES` 构造，不在此访问集合。
    //
    // 返回 `UnitOfMeasureRepository<'_>`。
    fn unit_of_measures(&self) -> UnitOfMeasureRepository<'_> {
        UnitOfMeasureRepository::new(self, Self::UNIT_OF_MEASURES)
    }

    // 绑定 `sku_attribute` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `SKU_ATTRIBUTES` 构造，不在此访问集合。
    //
    // 返回 `SkuAttributeRepository<'_>`。
    fn sku_attributes(&self) -> SkuAttributeRepository<'_> {
        SkuAttributeRepository::new(self, Self::SKU_ATTRIBUTES)
    }

    // 绑定 `sku_attribute_value` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `SKU_ATTRIBUTE_VALUES` 构造，不在此访问集合。
    //
    // 返回 `SkuAttributeValueRepository<'_>`。
    fn sku_attribute_values(&self) -> SkuAttributeValueRepository<'_> {
        SkuAttributeValueRepository::new(self, Self::SKU_ATTRIBUTE_VALUES)
    }

    // 绑定 `product_category_attribute` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `PRODUCT_CATEGORY_ATTRIBUTES` 构造，不在此访问集合。
    //
    // 返回 `ProductCategoryAttributeRepository<'_>`。
    fn product_category_attributes(&self) -> ProductCategoryAttributeRepository<'_> {
        ProductCategoryAttributeRepository::new(self, Self::PRODUCT_CATEGORY_ATTRIBUTES)
    }

    // 绑定 `product` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `PRODUCTS` 构造，不在此访问集合。
    //
    // 返回 `ProductRepository<'_>`。
    fn products(&self) -> ProductRepository<'_> {
        ProductRepository::new(self, Self::PRODUCTS)
    }

    // 绑定 `product_revision` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `PRODUCT_REVISIONS` 构造，不在此访问集合。
    //
    // 返回 `ProductRevisionRepository<'_>`。
    fn product_revisions(&self) -> ProductRevisionRepository<'_> {
        ProductRevisionRepository::new(self, Self::PRODUCT_REVISIONS)
    }

    // 绑定 `product_revision_media` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `PRODUCT_REVISION_MEDIAS` 构造，不在此访问集合。
    //
    // 返回 `ProductRevisionMediaRepository<'_>`。
    fn product_revision_medias(&self) -> ProductRevisionMediaRepository<'_> {
        ProductRevisionMediaRepository::new(self, Self::PRODUCT_REVISION_MEDIAS)
    }

    // 绑定 `sku` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `SKUS` 构造，不在此访问集合。
    //
    // 返回 `SkuRepository<'_>`。
    fn skus(&self) -> SkuRepository<'_> {
        SkuRepository::new(self, Self::SKUS)
    }

    // 绑定 `sku_revision` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `SKU_REVISIONS` 构造，不在此访问集合。
    //
    // 返回 `SkuRevisionRepository<'_>`。
    fn sku_revisions(&self) -> SkuRevisionRepository<'_> {
        SkuRevisionRepository::new(self, Self::SKU_REVISIONS)
    }

    fn sku_barcode_claims(&self) -> SkuBarcodeClaimRepository<'_> {
        SkuBarcodeClaimRepository::new(self, Self::SKU_BARCODE_CLAIMS)
    }

    // 绑定 `sku_revision_attribute_value` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `SKU_REVISION_ATTRIBUTE_VALUES` 构造，不在此访问集合。
    //
    // 返回 `persistence_core::Repository<'_, crate::entity::catalog::SkuRevisionAttributeValue>`。
    fn sku_revision_attribute_values(&self) -> persistence_core::Repository<'_, SkuRevisionAttributeValue> {
        persistence_core::Repository::new(self, Self::SKU_REVISION_ATTRIBUTE_VALUES)
    }

    // 绑定 `voucher_category_profile_revision` 集合仓储，供领域查询或写入步骤复用。
    //
    // 以当前数据库和集合常量 `VOUCHER_CATEGORY_PROFILE_REVISIONS` 构造，不在此访问集合。
    //
    // 返回 `VoucherCategoryProfileRevisionRepository<'_>`。
    fn voucher_category_profile_revisions(&self) -> VoucherCategoryProfileRevisionRepository<'_> {
        VoucherCategoryProfileRevisionRepository::new(self, Self::VOUCHER_CATEGORY_PROFILE_REVISIONS)
    }

    // 绑定本域跨集合用例的仓储；读写和事务边界仍由调用方传入的执行器决定。
    //
    // 以当前数据库构造 `CatalogRepository`，不在此开启事务或访问集合。
    //
    // 返回 `CatalogRepository` 实例。
    fn catalog(&self) -> CatalogRepository<'_> {
        CatalogRepository::new(self)
    }

    fn sellable_sku_projections(&self) -> SellableSkuProjectionStore<'_> {
        SellableSkuProjectionStore::new(self)
    }
}
