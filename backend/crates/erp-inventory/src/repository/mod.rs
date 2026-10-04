//! Inventory MongoDB repositories and accessors.

mod cancellation;
pub mod extensions;
pub use cancellation::StockAdjustmentCancellationExt;
pub mod inventory;
pub mod owned;
pub mod prelude;

mod fulfillment_facts;

pub use extensions::InventoryExt;
pub use fulfillment_facts::StockReservationRepositoryFulfillmentExt;
pub use inventory::{
    AdjustmentSnapshotReadFilter, InventoryRepository, InventorySearch, StockAdjustmentFilter,
    StockAdjustmentLineRepositoryExt, StockAdjustmentRepositoryExt, StockAdjustmentRow, StockBalanceFilter,
    StockBalanceRepositoryExt, StockBalanceRow, StockMovementFilter, StockMovementRepositoryExt,
    StockMovementRow, StockReservationFilter, StockReservationRepositoryExt, StockReservationRow,
};
pub use owned::{
    StockAdjustmentLineRepository, StockAdjustmentRepository, StockBalanceRepository,
    StockMovementRepository, StockReservationRepository,
};

#[cfg(test)]
mod bson_roundtrip;
