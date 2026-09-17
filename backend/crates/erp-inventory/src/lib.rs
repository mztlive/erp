//! Inventory domain: stock balances, movements, reservations and adjustments.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::*;
pub use entity::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationEntryType, ReservationStatus,
    StockAdjustment, StockAdjustmentApprovalSnapshot, StockAdjustmentData, StockAdjustmentId,
    StockAdjustmentLine, StockAdjustmentLineData, StockAdjustmentLineId, StockAdjustmentLineUpdate,
    StockAdjustmentSnapshotFact, StockAdjustmentState, StockAdjustmentUpdate, StockBalance, StockBalanceData,
    StockBalanceId, StockBalanceUpdate, StockMovement, StockMovementData, StockMovementId, StockReservation,
    StockReservationData, StockReservationEntry, StockReservationEntryData, StockReservationEntryId,
    StockReservationId, StockReservationSourceType, StockReservationUpdate,
};
pub use error::{Error, Result};
pub use ports::{
    AdjustmentPeopleFact, AdjustmentPeopleFactsPort, AuthorizationPort, CatalogFactsPort,
    FailClosedAdjustmentPeopleFactsPort, FailClosedAuditPort, FailClosedAuthorizationPort,
    FailClosedCatalogFacts, FailClosedFulfillmentFacts, FailClosedWarehouseFacts, FulfillmentFactsPort,
    InventoryAuditPort, InventoryAuthorization, InventoryScopeMeta, PreparedInventoryAudit, ReceiptNoFact,
    SkuFact, SkuRevisionFact, WarehouseFact, WarehouseFactsPort, WarehouseRevisionFact, WarehouseScope,
    applicant_object_ids, intersect_object_ids, latest_snapshot_submitters, merge_adjustment_people,
};
pub use repository::{
    AdjustmentSnapshotReadFilter, InventoryExt, InventoryRepository, StockAdjustmentFilter,
    StockAdjustmentLineRepository, StockAdjustmentRepository, StockAdjustmentRow, StockBalanceFilter,
    StockBalanceRepository, StockBalanceRow, StockMovementFilter, StockMovementRepository, StockMovementRow,
    StockReservationFilter, StockReservationRepository, StockReservationRow,
};
pub use service::{InventoryService, apply_posted_adjustment_in_transaction, build_adjustment_line_updates};
