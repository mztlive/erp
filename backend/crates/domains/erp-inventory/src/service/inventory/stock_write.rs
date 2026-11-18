//! 已过账库存调整的事务内库存写入。

use application_core::AuditActor;
use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{SkuId, StockMovementId, StockReservationEntryId, WarehouseId};
use erp_core::money::Quantity;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::inventory::{
    MovementDirection, ReservationEntryType, StockAdjustment, StockAdjustmentLine, StockMovement,
    StockMovementData, StockReservationEntry, StockReservationEntryData, zero_quantity,
};
use crate::error::{Error, Result};
use crate::repository::InventoryExt;
use crate::repository::prelude::*;

/// 调整单通过审批校验后，沿调用方事务逐行核对原因和方向，写库存流水、余额及适用的预占释放。仅在全部明细通过后标记调整单已过账。
/// 空明细返回 `ValidationError`；方向、流水或分录不合法及状态迁移失败返回 `Logic`。
/// 余额缺失、可用量不足、预占与余额不一致或释放数量超精度返回 `BusinessLogicError`；传播仓储错误。
pub async fn apply_posted_adjustment(
    db: &Database,
    adjustment: &mut StockAdjustment,
    lines: &[StockAdjustmentLine],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if lines.is_empty() {
        return Err(Error::ValidationError("库存调整单没有明细，无法过账".to_string()));
    }

    let occurred_at = Instant::now();
    for line in lines {
        adjustment.reason_type.ensure_direction(line.direction)?;
        post_adjustment_line(db, executor, adjustment, line, &occurred_at, actor).await?;
    }

    adjustment.mark_posted()?;
    db.stock_adjustments().update(adjustment, executor).await?;
    Ok(())
}

// 过账当前明细时在原事务登记变动流水，再按调整仓库和 SKU 更新余额；扣减分支先释放适用预占。
// 数量更新后登记该流水为余额的最后流水；余额缺失、可用量不足或写入失败时中止。
async fn post_adjustment_line(
    db: &Database,
    executor: &mut dyn Executor,
    adjustment: &StockAdjustment,
    line: &StockAdjustmentLine,
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let movement_type = adjustment.reason_type.movement_type();
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id: adjustment.warehouse_id.clone(),
            sku_id: line.sku_id.clone(),
            movement_type,
            direction: line.direction,
            quantity: line.quantity,
            source_document_id: adjustment.base.id.clone(),
            source_line_id: Some(line.base.id.clone()),
            reversal_of_movement_id: None,
            fact_no: next_id(),
            occurred_at: adjustment.occurred_at.unwrap_or(*occurred_at),
            recorded_at: *occurred_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: Some(adjustment.reason_type.as_str().to_string()),
            reason_text: adjustment.note.clone(),
        },
    )?;
    db.stock_movements().create(&movement, executor).await?;

    let balance = db
        .stock_balances()
        .find_by_dimensions(&adjustment.warehouse_id, &line.sku_id, executor)
        .await?
        .ok_or_else(|| {
            Error::BusinessLogicError(format!(
                "库存余额不存在（仓库 {}，SKU {}），请先建立期初或入库",
                adjustment.warehouse_id.as_ref(),
                line.sku_id.as_ref()
            ))
        })?;
    match line.direction {
        MovementDirection::Increase => {
            if !db.stock_balances().increase_on_hand(&balance.base.id, line.quantity, executor).await? {
                return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
            }
        },
        MovementDirection::Decrease => {
            release_applicable_reservations(
                db,
                executor,
                &adjustment.warehouse_id,
                &line.sku_id,
                &balance.base.id,
                line,
            )
            .await?;
            if !db.stock_balances().deduct_available(&balance.base.id, line.quantity, executor).await? {
                return Err(Error::BusinessLogicError("可用库存不足，无法过账库存调整".to_string()));
            }
        },
    }

    // 余额记录最后流水（台账「最后变动」列），与数量增减同事务
    if !db.stock_balances().apply_last_movement(&balance.base.id, &movement.base.id, executor).await? {
        return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
    }
    Ok(())
}

// 盘亏或损坏扣减前，在原事务按最早优先全额释放调整仓库/SKU 中的适用预占，达到或超过本次扣减量即停止。
// 条件写未命中则跳过该条；释放成功后同步余额预占并登记释放流水。余额与预占不一致、数量精度超限或写入失败时返回错误。
async fn release_applicable_reservations(
    db: &Database,
    executor: &mut dyn Executor,
    warehouse_id: &WarehouseId,
    sku_id: &SkuId,
    balance_id: &str,
    line: &StockAdjustmentLine,
) -> Result<()> {
    let reservations = db.inventory().oldest_operable_reservations(warehouse_id, sku_id, executor).await?;
    let zero = zero_quantity().to_decimal();
    let mut released_total = zero;
    let target = line.quantity.to_decimal();
    for reservation in reservations {
        if released_total >= target {
            break;
        }
        let remaining = reservation.reserved_quantity.to_decimal();
        if remaining <= zero {
            continue;
        }

        if !db
            .stock_reservations()
            .release_quantity(&reservation.base.id, reservation.reserved_quantity, executor)
            .await?
        {
            continue;
        }
        // 同步余额预占（释放量从 reserved 转入 available），否则后续
        // deduct_available 会因可用量不足而误拒盘亏/损坏过账。
        if !db.stock_balances().release_reserved(balance_id, reservation.reserved_quantity, executor).await? {
            return Err(Error::BusinessLogicError(
                "库存余额预占与预占记录不一致，无法过账库存调整".to_string(),
            ));
        }

        db.stock_reservation_entries()
            .create(
                &StockReservationEntry::new(
                    StockReservationEntryId::new(next_id()),
                    StockReservationEntryData {
                        reservation_id: reservation.base.id.into(),
                        entry_type: ReservationEntryType::Release,
                        quantity: reservation.reserved_quantity,
                        source_document_id: line.stock_adjustment_id.to_string(),
                    },
                )?,
                executor,
            )
            .await?;
        released_total = Quantity::try_from(released_total + remaining)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?
            .to_decimal();
    }
    Ok(())
}
