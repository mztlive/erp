//! Cross-domain processes: approval dispatch, audited transactions and named use cases.

mod errors;

pub use errors::{Error, Result};

pub mod adapters;
pub mod approval_dispatch;
pub mod attachments;
pub mod audit;
pub mod catalog;
pub mod contract;
pub mod customer;
pub mod customer_profile;
pub mod finance_posting;
pub mod import_apply;
pub mod inventory_adjustment;
pub mod party;
pub mod source_registry;
pub mod supplier;
pub mod supplier_profile;
pub mod warehouse;

pub use approval_dispatch::ApprovalActionRegistry;
pub use attachments::{
    commit_supplier_payment_with_assets, confirm_electronic_delivery_with_assets,
    confirm_service_fulfillment_with_assets, product_brand_create_with_assets,
    product_brand_update_with_assets, product_create_with_assets, product_update_with_assets,
    supplier_profile_create_with_assets, supplier_profile_update_with_assets,
};
pub use audit::run_audited;
pub use catalog::{
    create_product_category, create_sku_attribute, create_sku_attribute_value, create_unit_of_measure,
};
pub use contract::upload_contract;
pub use customer::delete_customer;
pub use customer_profile::CustomerProfileService;
pub use import_apply::ImportApplyService;
pub use inventory_adjustment::InventoryAdjustmentService;
pub use party::delete_party;
pub use source_registry::create_source_system;
pub use supplier::delete_supplier;
pub use supplier_profile::{SupplierProfileService, SupplierProfileWithAssetsResult};
pub use warehouse::create_warehouse_sku_policy;

pub mod order_to_cash;
pub mod procure_to_pay;
pub mod reverse_flow;
pub mod sales_change;
pub mod sales_selection;

pub mod fulfillment_execution;

pub mod integration_resolution;
pub mod supplier_connection_execution;

pub mod supply_execution;
pub mod supply_governance;
pub mod supply_settlement;

#[cfg(test)]
mod test_indexes;
