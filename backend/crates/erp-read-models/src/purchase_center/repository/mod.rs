//! 采购视图和命令 Port 共用的跨域事实来源；不包含业务写入或事务根。
mod basis_sources;
mod center_facts;
mod coverage;
mod coverage_service;
mod creation_basis;
pub(super) mod list_facts;
pub(crate) mod mapping;
pub mod supplier_names;

pub use basis_sources::{
    basis_groups_and_facts, basis_groups_for_order, load_effective_sales_order, stock_basis_groups_for_order,
};
pub use center_facts::{load_purchase_order_center_facts, PurchaseOrderCenterFacts, PurchasePayableFact};
pub use coverage::load_procurement_coverage_facts;
pub use coverage_service::load_sales_procurement_coverage;
pub use creation_basis::load_creation_basis_facts;
pub use list_facts::{load_purchase_order_list_page, PurchaseOrderListFacts};
pub use mapping::{sales_order_basis_fact, stock_balance_fact};
