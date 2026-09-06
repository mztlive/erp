//! Named attachment processes: pending file batches and business writes that persist them.

mod catalog;
mod fulfillment;
mod payable;
mod pending;
mod supplier;

pub use catalog::{
    product_brand_create_with_assets, product_brand_update_with_assets, product_create_with_assets,
    product_update_with_assets,
};
pub use fulfillment::confirm_service_fulfillment_with_assets;
pub use payable::commit_supplier_payment_with_assets;
pub use pending::PendingFileAssets;
pub use supplier::{supplier_profile_create_with_assets, supplier_profile_update_with_assets};

/// Process module name.
pub fn process_name() -> &'static str {
    "attachments"
}
