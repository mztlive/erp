//! 采购集合名与仓储工厂。

mod command_receipt;
mod procurement_responsibility;
mod purchase_order;

pub use command_receipt::PurchaseCommandExt;
pub use procurement_responsibility::ProcurementResponsibilityExt;
pub use purchase_order::PurchaseOrderExt;
