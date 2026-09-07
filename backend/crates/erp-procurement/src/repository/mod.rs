//! 采购集合的拥有仓储与事务内查询。

pub mod extensions;
pub mod owned;
pub mod procurement_responsibility;
pub mod purchase_order;

pub use extensions::{ProcurementResponsibilityExt, PurchaseOrderExt};
