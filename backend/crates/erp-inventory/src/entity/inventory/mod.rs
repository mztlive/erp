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

pub mod approval_snapshot;
pub mod stock_adjustment;
pub mod stock_balance;
pub mod stock_movement;
pub mod stock_reservation;

pub use approval_snapshot::{StockAdjustmentApprovalSnapshot, StockAdjustmentSnapshotFact};
pub use erp_core::ids::{
    StockAdjustmentId, StockAdjustmentLineId, StockBalanceId, StockMovementId, StockReservationEntryId,
    StockReservationId,
};
use erp_core::money::Quantity;
use erp_core::{Error, Result};
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

/// 返回库存域数量零值（定点 `0`，不做字符串解析）。
///
/// # 返回
/// 返回精确零数量。
pub fn zero_quantity() -> Quantity {
    Quantity::try_from(rust_decimal::Decimal::ZERO).expect("Decimal::ZERO 恒满足数量精度")
}

/// 校验数量为正数（方向单独表达的流水与明细共用）。
///
/// # 参数
/// * `quantity` - 待校验数量
/// * `message` - 数量非正时的错误文案（各实体保留原有面向用户提示）
///
/// # 返回
/// 数量为正时返回 `Ok(())`。
///
/// # 错误
/// 数量为零或负数时返回 `message`。
pub(crate) fn ensure_positive_quantity(quantity: Quantity, message: &str) -> Result<()> {
    if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from(message.to_string()));
    }
    Ok(())
}

/// 校验三个数量均非负（余额三元组与预占三数量共用）。
///
/// # 参数
/// * `first` - 第一个数量
/// * `second` - 第二个数量
/// * `third` - 第三个数量
/// * `message` - 任一数量为负时的错误文案（各实体保留原有面向用户提示）
///
/// # 返回
/// 全部非负时返回 `Ok(())`。
///
/// # 错误
/// 任一数量为负时返回 `message`。
pub(crate) fn ensure_non_negative_quantities(
    first: Quantity,
    second: Quantity,
    third: Quantity,
    message: &str,
) -> Result<()> {
    let zero = rust_decimal::Decimal::ZERO;
    if first.to_decimal() < zero || second.to_decimal() < zero || third.to_decimal() < zero {
        return Err(Error::from(message.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::Quantity;

    use super::{ensure_non_negative_quantities, ensure_positive_quantity, zero_quantity};

    #[test]
    fn shared_quantity_helpers_keep_positive_and_non_negative_semantics() {
        assert_eq!(zero_quantity(), Quantity::from_str("0").unwrap());
        assert!(ensure_positive_quantity(Quantity::from_str("1").unwrap(), "正数").is_ok());
        assert!(ensure_positive_quantity(zero_quantity(), "必须为正数").is_err());
        assert!(
            ensure_non_negative_quantities(zero_quantity(), zero_quantity(), zero_quantity(), "非负").is_ok()
        );
        assert!(
            ensure_non_negative_quantities(
                Quantity::from_str("-1").unwrap(),
                zero_quantity(),
                zero_quantity(),
                "非负"
            )
            .is_err()
        );
    }
}
