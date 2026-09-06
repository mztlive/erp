//! Cross-domain processes: approval dispatch, audited transactions and named use cases.

pub mod approval_dispatch;
pub mod attachments;
pub mod audit;
pub mod catalog;
pub mod customer;
pub mod party;
pub mod source_registry;
pub mod supplier;
pub mod warehouse;

pub use approval_dispatch::ApprovalActionRegistry;
pub use attachments::{
    commit_supplier_payment_with_assets, confirm_service_fulfillment_with_assets,
    product_brand_create_with_assets, product_brand_update_with_assets, product_create_with_assets,
    product_update_with_assets, supplier_profile_create_with_assets, supplier_profile_update_with_assets,
};
pub use audit::run_audited;
pub use catalog::{
    create_product_category, create_sku_attribute, create_sku_attribute_value, create_unit_of_measure,
};
pub use customer::delete_customer;
pub use party::delete_party;
pub use source_registry::create_source_system;
pub use supplier::delete_supplier;
pub use warehouse::create_warehouse_sku_policy;
