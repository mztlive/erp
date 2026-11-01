//! 库存应用服务：查询与事务内库存写入。

pub mod inventory;

pub use inventory::{InventoryService, apply_posted_adjustment, build_adjustment_line_updates};

pub mod fulfillment;
