//! 供应链拥有仓储及窄访问器。

mod extensions;
pub mod owned;
pub mod supplier_api;
pub mod supplier_fulfillment;
pub mod supplier_offering;
pub mod supplier_settlement;

pub use extensions::{SupplierApiExt, SupplierFulfillmentExt, SupplierOfferingExt, SupplierSettlementExt};
pub use supplier_offering::{
    OfferingReadScope, OfferingScopeClause, SupplierOfferingFilter, SupplierOfferingRow,
};
