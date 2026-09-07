//! Owned inventory repositories composed from persistence-core.

mod stock_adjustment;
mod stock_adjustment_line;
mod stock_balance;
mod stock_movement;
mod stock_reservation;

pub use stock_adjustment::StockAdjustmentRepository;
pub use stock_adjustment_line::StockAdjustmentLineRepository;
pub use stock_balance::StockBalanceRepository;
pub use stock_movement::StockMovementRepository;
pub use stock_reservation::StockReservationRepository;
