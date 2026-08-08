//! 域 D24 `supplier_catalog` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCTS` 等值。

use entities::supplier_catalog::{
    SupplierCatalogCommand, SupplierCatalogIntakeBatch, SupplierCatalogIntakeItem, SupplierCatalogProduct,
    SupplierCatalogProductRevision, SupplierCatalogProductRevisionMedia, SupplierCatalogSku,
    SupplierCatalogSkuRevision, SupplierOffering, SupplierOfferingRevision, SupplierProductMapping,
};
use mongodb::Database;

use super::super::supplier_catalog::{
    SupplierCatalogIntakeBatchFilter, SupplierCatalogProductFilter, SupplierCatalogRepository,
    SupplierCatalogSkuFilter, SupplierOfferingFilter, SupplierProductMappingFilter,
};
use crate::Repository;

/// 域 D24 仓储访问器。
pub trait SupplierCatalogExt: Sized {
    /// `supplier_catalog_product` 集合名。
    const SUPPLIER_CATALOG_PRODUCTS: &'static str = "supplier_catalog_products";
    /// `supplier_catalog_product_revision` 集合名。
    const SUPPLIER_CATALOG_PRODUCT_REVISIONS: &'static str = "supplier_catalog_product_revisions";
    /// `supplier_catalog_product_revision_media` 集合名。
    const SUPPLIER_CATALOG_PRODUCT_REVISION_MEDIA: &'static str = "supplier_catalog_product_revision_media";
    /// `supplier_catalog_sku` 集合名。
    const SUPPLIER_CATALOG_SKUS: &'static str = "supplier_catalog_skus";
    /// `supplier_catalog_sku_revision` 集合名。
    const SUPPLIER_CATALOG_SKU_REVISIONS: &'static str = "supplier_catalog_sku_revisions";
    /// `supplier_product_mapping` 集合名。
    const SUPPLIER_PRODUCT_MAPPINGS: &'static str = "supplier_product_mappings";
    /// `supplier_catalog_intake_batch` 集合名。
    const SUPPLIER_CATALOG_INTAKE_BATCHES: &'static str = "supplier_catalog_intake_batches";
    /// `supplier_catalog_intake_item` 集合名。
    const SUPPLIER_CATALOG_INTAKE_ITEMS: &'static str = "supplier_catalog_intake_items";
    /// `supplier_offering` 集合名。
    const SUPPLIER_OFFERINGS: &'static str = "supplier_offerings";
    /// `supplier_offering_revision` 集合名。
    const SUPPLIER_OFFERING_REVISIONS: &'static str = "supplier_offering_revisions";
    /// `supplier_catalog_command` 集合名。
    const SUPPLIER_CATALOG_COMMANDS: &'static str = "supplier_catalog_commands";

    /// 供应商 SPU 列表筛选条件类型（定义见 `repository::supplier_catalog`）。
    type SupplierCatalogProductFilter;

    /// 供应商 SKU 列表筛选条件类型（定义见 `repository::supplier_catalog`）。
    type SupplierCatalogSkuFilter;

    /// 映射列表筛选条件类型（定义见 `repository::supplier_catalog`）。
    type SupplierProductMappingFilter;

    /// 入库批次列表筛选条件类型（定义见 `repository::supplier_catalog`）。
    type SupplierCatalogIntakeBatchFilter;

    /// 供给列表筛选条件类型（定义见 `repository::supplier_catalog`）。
    type SupplierOfferingFilter;

    /// 获取 `supplier_catalog_product` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogProduct>`。
    fn supplier_catalog_products(&self) -> Repository<'_, SupplierCatalogProduct>;

    /// 获取 `supplier_catalog_product_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogProductRevision>`。
    fn supplier_catalog_product_revisions(&self) -> Repository<'_, SupplierCatalogProductRevision>;

    /// 获取 `supplier_catalog_product_revision_media` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogProductRevisionMedia>`。
    fn supplier_catalog_product_revision_media(&self) -> Repository<'_, SupplierCatalogProductRevisionMedia>;

    /// 获取 `supplier_catalog_sku` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogSku>`。
    fn supplier_catalog_skus(&self) -> Repository<'_, SupplierCatalogSku>;

    /// 获取 `supplier_catalog_sku_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogSkuRevision>`。
    fn supplier_catalog_sku_revisions(&self) -> Repository<'_, SupplierCatalogSkuRevision>;

    /// 获取 `supplier_product_mapping` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierProductMapping>`。
    fn supplier_product_mappings(&self) -> Repository<'_, SupplierProductMapping>;

    /// 获取 `supplier_catalog_intake_batch` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogIntakeBatch>`。
    fn supplier_catalog_intake_batches(&self) -> Repository<'_, SupplierCatalogIntakeBatch>;

    /// 获取 `supplier_catalog_intake_item` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogIntakeItem>`。
    fn supplier_catalog_intake_items(&self) -> Repository<'_, SupplierCatalogIntakeItem>;

    /// 获取 `supplier_offering` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierOffering>`。
    fn supplier_offerings(&self) -> Repository<'_, SupplierOffering>;

    /// 获取 `supplier_offering_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierOfferingRevision>`。
    fn supplier_offering_revisions(&self) -> Repository<'_, SupplierOfferingRevision>;

    /// 获取供应商商品库写命令去重记录集合。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_catalog::SupplierCatalogCommand>`。
    fn supplier_catalog_commands(&self) -> Repository<'_, SupplierCatalogCommand>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SupplierCatalogRepository` 实例。
    fn supplier_catalog(&self) -> SupplierCatalogRepository<'_>;
}

impl SupplierCatalogExt for Database {
    type SupplierCatalogProductFilter = SupplierCatalogProductFilter;
    type SupplierCatalogSkuFilter = SupplierCatalogSkuFilter;
    type SupplierProductMappingFilter = SupplierProductMappingFilter;
    type SupplierCatalogIntakeBatchFilter = SupplierCatalogIntakeBatchFilter;
    type SupplierOfferingFilter = SupplierOfferingFilter;

    fn supplier_catalog_products(&self) -> Repository<'_, SupplierCatalogProduct> {
        Repository::new(self, Self::SUPPLIER_CATALOG_PRODUCTS)
    }

    fn supplier_catalog_product_revisions(&self) -> Repository<'_, SupplierCatalogProductRevision> {
        Repository::new(self, Self::SUPPLIER_CATALOG_PRODUCT_REVISIONS)
    }

    fn supplier_catalog_product_revision_media(&self) -> Repository<'_, SupplierCatalogProductRevisionMedia> {
        Repository::new(self, Self::SUPPLIER_CATALOG_PRODUCT_REVISION_MEDIA)
    }

    fn supplier_catalog_skus(&self) -> Repository<'_, SupplierCatalogSku> {
        Repository::new(self, Self::SUPPLIER_CATALOG_SKUS)
    }

    fn supplier_catalog_sku_revisions(&self) -> Repository<'_, SupplierCatalogSkuRevision> {
        Repository::new(self, Self::SUPPLIER_CATALOG_SKU_REVISIONS)
    }

    fn supplier_product_mappings(&self) -> Repository<'_, SupplierProductMapping> {
        Repository::new(self, Self::SUPPLIER_PRODUCT_MAPPINGS)
    }

    fn supplier_catalog_intake_batches(&self) -> Repository<'_, SupplierCatalogIntakeBatch> {
        Repository::new(self, Self::SUPPLIER_CATALOG_INTAKE_BATCHES)
    }

    fn supplier_catalog_intake_items(&self) -> Repository<'_, SupplierCatalogIntakeItem> {
        Repository::new(self, Self::SUPPLIER_CATALOG_INTAKE_ITEMS)
    }

    fn supplier_offerings(&self) -> Repository<'_, SupplierOffering> {
        Repository::new(self, Self::SUPPLIER_OFFERINGS)
    }

    fn supplier_offering_revisions(&self) -> Repository<'_, SupplierOfferingRevision> {
        Repository::new(self, Self::SUPPLIER_OFFERING_REVISIONS)
    }

    fn supplier_catalog_commands(&self) -> Repository<'_, SupplierCatalogCommand> {
        Repository::new(self, Self::SUPPLIER_CATALOG_COMMANDS)
    }

    fn supplier_catalog(&self) -> SupplierCatalogRepository<'_> {
        SupplierCatalogRepository::new(self)
    }
}
