//! 域 D24 `supplier_catalog`：supplier_catalog_product(+_revision、_revision_media)、supplier_catalog_sku、supplier_product_mapping、supplier_catalog_intake_batch、supplier_offering（页面：W21）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D24 仓储访问器（P2 填充）。
pub trait SupplierCatalogExt: Sized {}

impl SupplierCatalogExt for mongodb::Database {}
