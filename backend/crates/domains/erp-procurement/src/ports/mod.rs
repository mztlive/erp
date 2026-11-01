//! 采购公开合同。

pub mod change;
pub mod coverage;
pub mod creation_basis;
pub mod data_scope;
pub mod procurement_responsibility;
pub mod purchase_order;

pub use data_scope::{
    FailClosedPurchaseDataScopePort, PurchaseDataScopePort, PurchaseResolvedClause, PurchaseResolvedScope,
    PurchaseScopeObject,
};
