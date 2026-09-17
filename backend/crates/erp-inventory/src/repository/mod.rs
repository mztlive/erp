//! Inventory MongoDB repositories and accessors.

pub mod extensions;
pub mod inventory;
pub mod owned;

pub use extensions::InventoryExt;
pub use inventory::{
    AdjustmentSnapshotReadFilter, InventoryRepository, InventorySearch, StockAdjustmentFilter,
    StockAdjustmentRow, StockBalanceFilter, StockBalanceRow, StockMovementFilter, StockMovementRow,
    StockReservationFilter, StockReservationRow,
};
pub use owned::{
    StockAdjustmentLineRepository, StockAdjustmentRepository, StockBalanceRepository,
    StockMovementRepository, StockReservationRepository,
};

#[cfg(test)]
mod bson_roundtrip;

mod fulfillment_facts;
