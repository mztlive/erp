//! Inventory application services for queries and in-transaction stock writes.

pub mod inventory;

pub use inventory::{
    apply_posted_adjustment_in_transaction, build_adjustment_line_updates, InventoryService,
};
