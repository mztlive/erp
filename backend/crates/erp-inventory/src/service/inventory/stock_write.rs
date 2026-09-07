//! In-transaction stock writes for posted inventory adjustments.

use std::str::FromStr;

use crate::entity::inventory::{
    MovementDirection, ReservationEntryType, StockAdjustment, StockAdjustmentLine, StockMovement,
    StockMovementData, StockReservationEntry, StockReservationEntryData,
};
use crate::error::{Error, Result};
use crate::repository::InventoryExt;
use application_core::AuditActor;
use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{StockMovementId, StockReservationEntryId};
use erp_core::money::Quantity;
use id_generator::next_id;
use mongodb::Database;

/// 在调用方事务内写入过账流水、余额与预占释放，并把调整单标为已过账。
///
/// # 参数
/// * `db` - 数据库实例
/// * `adjustment` - 已通过审批校验的调整单
/// * `lines` - 调整明细
/// * `actor` - 已认证操作人
/// * `session` - 调用方持有的事务会话
///
/// # 错误
/// 明细为空、方向不匹配、余额不足或任一库存写入失败。
pub async fn apply_posted_adjustment_in_transaction(
    db: &Database,
    adjustment: &mut StockAdjustment,
    lines: &[StockAdjustmentLine],
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    if lines.is_empty() {
        return Err(Error::ValidationError("库存调整单没有明细，无法过账".to_string()));
    }
    let occurred_at = Instant::now();
    for line in lines {
        adjustment.reason_type.ensure_direction(line.direction)?;
        post_adjustment_line(db, session, adjustment, line, &occurred_at, actor).await?;
    }
    adjustment.mark_posted()?;
    db.stock_adjustments().update(adjustment, session).await?;
    Ok(())
}

/// 过账单条调整明细（流水 + 余额 + 适用预占释放，位于调用方事务内）。
async fn post_adjustment_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
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
    db.stock_movements().create(&movement, session).await?;

    let balance = db
        .inventory()
        .balance_for_dimensions(&adjustment.warehouse_id, &line.sku_id, session)
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
            if !db
                .stock_balances()
                .increase_on_hand(&balance.base.id, line.quantity, session)
                .await?
            {
                return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
            }
        }
        MovementDirection::Decrease => {
            release_applicable_reservations(
                db,
                session,
                &adjustment.warehouse_id,
                &line.sku_id,
                &balance.base.id,
                line,
            )
            .await?;
            if !db
                .stock_balances()
                .deduct_available(&balance.base.id, line.quantity, session)
                .await?
            {
                return Err(Error::BusinessLogicError(
                    "可用库存不足，无法过账库存调整".to_string(),
                ));
            }
        }
    }
    if !db
        .stock_balances()
        .apply_last_movement(&balance.base.id, &movement.base.id, session)
        .await?
    {
        return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
    }
    Ok(())
}

/// 释放调整仓库/SKU 上的适用预占（盘亏/损坏扣减前）。
async fn release_applicable_reservations(
    db: &Database,
    session: &mut mongodb::ClientSession,
    warehouse_id: &erp_core::ids::WarehouseId,
    sku_id: &erp_core::ids::SkuId,
    balance_id: &str,
    line: &StockAdjustmentLine,
) -> Result<()> {
    let reservations = db
        .inventory()
        .oldest_operable_reservations(warehouse_id, sku_id, session)
        .await?;
    let mut released_total = Quantity::from_str("0").unwrap();
    let target = line.quantity.to_decimal();
    for reservation in reservations {
        if released_total.to_decimal() >= target {
            break;
        }
        let remaining = reservation.reserved_quantity.to_decimal();
        if remaining <= Quantity::from_str("0").unwrap().to_decimal() {
            continue;
        }
        if !db
            .stock_reservations()
            .release_quantity(&reservation.base.id, reservation.reserved_quantity, session)
            .await?
        {
            continue;
        }
        if !db
            .stock_balances()
            .release_reserved(balance_id, reservation.reserved_quantity, session)
            .await?
        {
            return Err(Error::BusinessLogicError(
                "库存余额预占与预占记录不一致，无法过账库存调整".to_string(),
            ));
        }
        db.stock_reservation_entries()
            .create(
                &StockReservationEntry::new(
                    StockReservationEntryId::new(next_id()),
                    StockReservationEntryData {
                        reservation_id: reservation.base.id.clone().into(),
                        entry_type: ReservationEntryType::Release,
                        quantity: reservation.reserved_quantity,
                        source_document_id: line.stock_adjustment_id.to_string(),
                    },
                )?,
                session,
            )
            .await?;
        released_total = Quantity::try_from(released_total.to_decimal() + remaining)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    }
    Ok(())
}
