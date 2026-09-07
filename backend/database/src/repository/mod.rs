//! MongoDB仓储实现模块
//!
//! 提供基于MongoDB的数据访问层实现

pub mod extensions;
mod fulfillment;
mod integration_ops;

pub mod owned;
mod procurement_responsibility;
mod purchase_order;
mod returns;
mod sales_order;
mod sales_review;
mod supplier_api;
mod supplier_fulfillment;
mod supplier_names;
mod supplier_offering;
mod supplier_settlement;

pub use extensions::DatabaseExt;
pub use owned::*;
pub use procurement_responsibility::ProcurementResponsibilityRuleFilter;
pub use supplier_names::current_legal_names_by_account_ids;
pub use supplier_offering::SupplierOfferingRow;
