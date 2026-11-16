//! 主体、客户、供应商、商品、仓库与合同主数据的跨域命令。
use persistence_core::Error as PersistenceError;

mod errors;
mod handover_common;
pub use errors::{Error, Result};
pub mod attachments;
pub mod catalog;
pub mod contract;
pub mod customer;
pub mod customer_profile;
pub mod offering_handover;
pub mod party;
pub mod product_handover;
pub mod supplier;
pub mod supplier_profile;
pub mod warehouse;
pub use attachments::*;
pub use catalog::{
    create_product_category, create_sku_attribute, create_sku_attribute_value, create_unit_of_measure,
};
pub use contract::upload_contract;
pub use customer::delete_customer;
pub use customer_profile::CustomerProfileProcess;
pub use offering_handover::{handover_candidates as offering_handover_candidates, handover_offering};
pub use party::delete_party;
pub use product_handover::{handover_candidates as product_handover_candidates, handover_product};
pub use supplier::delete_supplier;
pub use supplier_profile::{SupplierProfileProcess, SupplierProfileWithAssetsResult};
pub use warehouse::create_warehouse_sku_policy;

/// 只有命令唯一键冲突允许按已提交回执执行只读恢复。
fn command_insert_error(error: PersistenceError) -> Error {
    match error {
        error @ PersistenceError::DuplicateKey(_) => Error::ReceiptDuplicate(error),
        other => other.into(),
    }
}
