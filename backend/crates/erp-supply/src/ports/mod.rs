//! 供应链消费的外部事实和网关合同。

pub mod data_scope;
pub mod offering_qualification;
pub mod settlement_data_scope;
pub mod supplier_api_gateway;
pub mod supplier_gateway;
pub mod supplier_reference_registry;

pub use data_scope::{
    FailClosedOfferingDataScopePort, OfferingDataScopePort, OfferingResolvedClause, OfferingResolvedScope,
    OfferingScopeObject,
};
pub use settlement_data_scope::{
    FailClosedSettlementDataScopePort, SettlementDataScopePort, SettlementResolvedClause,
    SettlementResolvedScope, SettlementScopeObject,
};
