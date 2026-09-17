//! 供应链领域各组公开合同。

pub mod offering_scope;
pub mod supplier_api;
pub mod supplier_fulfillment;
pub mod supplier_fulfillment_scope;
pub use supplier_fulfillment_scope::{
    FulfillmentHandoverCandidateView, HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView,
    SupplierFulfillmentOrderListView,
};
pub mod supplier_offering;
pub mod supplier_settlement;

pub use offering_scope::{
    HandoverCandidateView, HandoverSupplierOfferingRequest, HandoverSupplierOfferingView,
};
