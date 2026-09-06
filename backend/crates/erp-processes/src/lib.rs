//! Cross-domain processes: approval dispatch, audited transactions and named use cases.

pub mod approval_dispatch;
pub mod audit;
pub mod catalog;
pub mod customer;
pub mod party;
pub mod source_registry;
pub mod supplier;
pub mod warehouse;

pub use approval_dispatch::ApprovalActionRegistry;
pub use audit::run_audited;
pub use catalog::{
    create_product_category, create_sku_attribute, create_sku_attribute_value, create_unit_of_measure,
};
pub use customer::delete_customer;
pub use party::delete_party;
pub use source_registry::create_source_system;
pub use supplier::delete_supplier;
pub use warehouse::create_warehouse_sku_policy;
