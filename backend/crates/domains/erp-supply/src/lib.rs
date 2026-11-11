//! 供应供给、连接能力、供应商履约和结算的领域规则与持久化。

pub mod after_sales;
pub mod api_order;
pub mod catalog_adoption;
pub mod catalog_membership;
pub mod command_receipt;
pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod inquiry;
pub mod order_tracking;
pub mod portal;
pub mod ports;
pub mod repository;
pub mod service;

pub use authorization::{FulfillmentOrderResolvedScope, OfferingResolvedScope, SettlementResolvedScope};
pub use dto::supplier_fulfillment_scope::{
    FulfillmentHandoverCandidateView, HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView,
    SupplierFulfillmentOrderListView,
};
pub use dto::{HandoverCandidateView, HandoverSupplierOfferingRequest, HandoverSupplierOfferingView};
pub use error::{Error, Result, known_duplicate_index_message};
pub use ports::{FailClosedFulfillmentExceptionHandlerPort, FulfillmentExceptionHandlerPort};
pub use repository::supplier_settlement::SettlementReadScope;
pub use repository::{
    FulfillmentOrderReadScope, OfferingReadScope, SupplierOfferingExt, SupplierOfferingFilter,
};
pub use service::supplier_fulfillment::FulfillmentOrderAccess;
pub use service::supplier_offering::{OfferingAccess, SupplierOfferingService};
pub use service::supplier_settlement::SettlementAccess;

pub mod connector_registry;

pub mod authorization;
