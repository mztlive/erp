//! 供应链集合的唯一访问器和集合名。

mod supplier_api;
pub use supplier_api::SupplierApiExt;
mod supplier_offering;
pub use supplier_offering::SupplierOfferingExt;
mod supplier_fulfillment;
pub use supplier_fulfillment::SupplierFulfillmentExt;
mod supplier_settlement;
pub use supplier_settlement::SupplierSettlementExt;
