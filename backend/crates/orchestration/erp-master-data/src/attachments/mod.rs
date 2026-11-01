//! 在主数据事务中提交文件资产的命名命令。
mod catalog;
mod supplier;
pub use catalog::{
    product_brand_create_with_assets, product_brand_update_with_assets, product_create_with_assets,
    product_update_with_assets,
};
pub use supplier::{supplier_profile_create_with_assets, supplier_profile_update_with_assets};
