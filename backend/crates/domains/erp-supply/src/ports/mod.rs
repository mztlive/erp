//! 供应链消费的外部事实和网关合同。

pub mod catalog_sync;
pub mod connector;
pub mod fulfillment_order_data_scope;
pub mod offering_qualification;
pub mod supplier_api_gateway;
pub mod supplier_gateway;
pub mod supplier_reference_registry;

pub use fulfillment_order_data_scope::{
    FailClosedFulfillmentExceptionHandlerPort, FulfillmentExceptionHandlerPort,
};

pub mod jd_vop;
