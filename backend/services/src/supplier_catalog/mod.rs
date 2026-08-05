//! 域 D24 `supplier_catalog`：supplier_catalog_product(+_revision、_revision_media)、supplier_catalog_sku、supplier_product_mapping、supplier_catalog_intake_batch、supplier_offering（页面：W21）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
