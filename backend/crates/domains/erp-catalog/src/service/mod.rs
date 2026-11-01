//! 商品域应用服务。

pub mod catalog;

pub use catalog::{
    CatalogAccess, CatalogService, SellableSkuListParams, SellableSkuSpecificationAttributeView,
    SellableSkuView, catalog_scope, sellable_sku_invalid_error,
};
