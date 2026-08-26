//! 域 D17 `inventory`：库存流水、余额、预占与库存调整（页面：W10 库存台账）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与约束见数据模型 §6.7；公共字段归属按 §4.3 判定：
//! - `stock_movement` 的字典含 `occurred_at`/`recorded_at` 等正式事实字段 →
//!   组合 `FactBase`，且**不可更新或删除**（§6.7）；`stock_reservation_entry`
//!   按字典精确建模（只组合 `BaseModel`），同为正式流水；
//! - 全部实体 `#[serde(flatten)] BaseModel`；正式流水与已过账单据不设业务
//!   软删除（§4.5.1），草稿可逻辑删除（§4.5.2）；
//! - 库存三元组不变式（§8.2 第 4 条：`on_hand >= 0`、`reserved >= 0`、
//!   `available = on_hand - reserved >= 0`）在 `stock_balance` 构造与更新时
//!   校验；`stock_movement` 的类型-方向语义按字典校验；跨聚合的余额联动、
//!   预占建立/消耗/释放与原数量上限校验由 P3 完成；
//! - 状态机按 §6.7/§7.5：`stock_adjustment` 为草稿 → 待仓储复核 → 待财务确认
//!   → 已过账 → 已冲正（复核/确认可驳回退回草稿），`REVERSED` 为不可逆终态。

pub mod stock_adjustment;
pub mod stock_balance;
pub mod stock_movement;
pub mod stock_reservation;

pub use crate::ids::{
    StockAdjustmentId, StockAdjustmentLineId, StockBalanceId, StockMovementId, StockReservationEntryId,
    StockReservationId,
};
pub use stock_adjustment::{
    AdjustmentReasonType, StockAdjustment, StockAdjustmentData, StockAdjustmentLine, StockAdjustmentLineData,
    StockAdjustmentLineUpdate, StockAdjustmentState, StockAdjustmentUpdate,
};
pub use stock_balance::{StockBalance, StockBalanceData, StockBalanceUpdate};
pub use stock_movement::{MovementDirection, MovementType, StockMovement, StockMovementData};
pub use stock_reservation::{
    ReservationEntryType, ReservationStatus, StockReservation, StockReservationData, StockReservationEntry,
    StockReservationEntryData, StockReservationSourceType, StockReservationUpdate,
};
