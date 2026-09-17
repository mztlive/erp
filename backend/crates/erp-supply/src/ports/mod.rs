//! 供应链消费的外部事实和网关合同。

pub mod offering_qualification;
pub mod settlement_data_scope;
pub mod supplier_api_gateway;
pub mod supplier_gateway;
pub mod supplier_reference_registry;

pub use settlement_data_scope::{
    FailClosedSettlementDataScopePort, SettlementDataScopePort, SettlementResolvedClause,
    SettlementResolvedScope, SettlementScopeObject,
};
