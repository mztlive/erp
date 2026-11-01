//! [`persistence_core::Repository`] 的按集合别名。
//!
//! 领域方法是泛型仓储上的扩展 trait。

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
