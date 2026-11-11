//! 履约、库存调整与供应执行的跨域命令及事务协调。

pub mod attachments;
mod authorization;
mod errors;
pub mod fulfillment_execution;
pub mod inventory_adjustment;
pub mod supply_execution;
pub mod workflow;

pub use attachments::{confirm_electronic_delivery_with_assets, confirm_service_fulfillment_with_assets};
pub use errors::{Error, Result};
pub use inventory_adjustment::InventoryAdjustmentProcess;
