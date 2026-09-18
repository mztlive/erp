//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type StockAdjustmentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::inventory::StockAdjustment>;

pub type StockAdjustmentLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::inventory::StockAdjustmentLine>;

pub type StockBalanceRepository<'a> =
    persistence_core::Repository<'a, crate::entity::inventory::StockBalance>;

pub type StockMovementRepository<'a> =
    persistence_core::Repository<'a, crate::entity::inventory::StockMovement>;

pub type StockReservationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::inventory::StockReservation>;
