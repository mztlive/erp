//! 附件流程：待持久化的文件批次，以及在同一业务事务中写入它们的命令。

mod catalog;
mod fulfillment;
mod payable;
mod pending;
mod supplier;

pub use catalog::{
    product_brand_create_with_assets, product_brand_update_with_assets, product_create_with_assets,
    product_update_with_assets,
};
pub use fulfillment::{confirm_electronic_delivery_with_assets, confirm_service_fulfillment_with_assets};
pub use payable::commit_supplier_payment_with_assets;
pub use pending::PendingFileAssets;
pub use supplier::{supplier_profile_create_with_assets, supplier_profile_update_with_assets};

/// 返回附件流程模块名。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回稳定模块名 `attachments`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "attachments"
}
