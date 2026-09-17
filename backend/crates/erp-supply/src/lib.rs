//! 供应供给、连接能力、供应商履约和结算的领域规则与持久化。

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use dto::supplier_fulfillment_scope::{
    FulfillmentHandoverCandidateView, HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView,
    SupplierFulfillmentOrderListView,
};
pub use dto::{HandoverCandidateView, HandoverSupplierOfferingRequest, HandoverSupplierOfferingView};
pub use error::{Error, Result, known_duplicate_index_message};
pub use ports::{
    FailClosedFulfillmentExceptionHandlerPort, FailClosedFulfillmentOrderDataScopePort,
    FailClosedOfferingDataScopePort, FailClosedSettlementDataScopePort, FulfillmentExceptionHandlerPort,
    FulfillmentOrderDataScopePort, FulfillmentOrderResolvedClause, FulfillmentOrderResolvedScope,
    FulfillmentOrderScopeObject, OfferingDataScopePort, OfferingResolvedClause, OfferingResolvedScope,
    OfferingScopeObject, SettlementDataScopePort, SettlementResolvedClause, SettlementResolvedScope,
    SettlementScopeObject,
};
pub use repository::supplier_settlement::{SettlementReadScope, SettlementScopeClause};
pub use repository::{
    FulfillmentOrderReadScope, FulfillmentOrderScopeClause, OfferingReadScope, OfferingScopeClause,
    SupplierOfferingExt, SupplierOfferingFilter,
};
pub use service::supplier_fulfillment::{FulfillmentOrderAccess, fulfillment_order_scope};
pub use service::supplier_offering::{OfferingAccess, SupplierOfferingService, offering_scope};
pub use service::supplier_settlement::{SettlementAccess, settlement_scope};
