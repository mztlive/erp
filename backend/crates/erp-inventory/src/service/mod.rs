//! Inventory application services for queries and in-transaction stock writes.

pub mod inventory;

pub use inventory::{InventoryService, apply_posted_adjustment, build_adjustment_line_updates};

pub mod fulfillment;
