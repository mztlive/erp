use database::{AccessControlExt, FulfillmentExt, InventoryExt, PurchaseOrderExt, Transactional};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::fulfillment::{Delivery, DeliveryLine, DeliveryState, DeliveryType};
use entities::ids::{DeliveryId, StockMovementId, StockReservationEntryId};
use entities::inventory::{
    MovementDirection, MovementType, ReservationEntryType, StockMovement, StockMovementData,
    StockReservationEntry, StockReservationEntryData,
};
use id_generator::next_id;
use mongodb::Database;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::purchase_context::{ensure_po_fulfillable, ensure_prepay_gate};
use super::{DeliveryView, FulfillmentService};

impl FulfillmentService {
    /// 过账发货（草稿 → 已发货；§8.2 第 2 条跨集合事务）。
    ///
    /// 仓发在同一事务内：校验预占归属（预占必须属于本销售明细且数量充足）、
    /// 消耗预占（含预占流水）、追加库存减少流水、更新库存余额、迁移发货单
    /// 状态、写审计。供应商直发不写自有库存流水，只做 `PREPAY` 门槛校验
    /// （§8.1.5）后迁移状态。重复过账由状态守卫（仅草稿）与流水唯一索引双重
    /// 防护。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的发货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单/预占/余额不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `BusinessLogicError` - 预占归属不符、数量不足或门槛未满足
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn post_delivery(&self, id: &str, actor: &AuditActor) -> Result<DeliveryView> {
        let delivery_id = DeliveryId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut delivery = db
                        .deliveries()
                        .find_by_id(delivery_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
                    if delivery.status != DeliveryState::Draft {
                        return Err(Error::ConflictError("只有草稿状态的发货单可以过账".to_string()));
                    }
                    let lines = db
                        .fulfillment()
                        .delivery_lines_by_delivery_ids(std::slice::from_ref(&delivery_id), session)
                        .await?;
                    if lines.is_empty() {
                        return Err(Error::ValidationError("发货单没有行，无法过账".to_string()));
                    }
                    let occurred_at = Instant::now();
                    match delivery.delivery_type {
                        DeliveryType::WarehouseShip => {
                            for line in &lines {
                                post_warehouse_ship_line(&db, session, &delivery, line, &occurred_at, &actor)
                                    .await?;
                            }
                        }
                        DeliveryType::SupplierDirect => {
                            let po = delivery.purchase_order_id.clone().ok_or_else(|| {
                                Error::BusinessLogicError("供应商直发缺少采购来源".to_string())
                            })?;
                            let po = db
                                .purchase_orders()
                                .find_by_id(po.as_ref(), session)
                                .await?
                                .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                            ensure_po_fulfillable(&po)?;
                            ensure_prepay_gate(&db, session, &po).await?;
                        }
                    }
                    delivery.mark_shipped(occurred_at)?;
                    db.deliveries().update(&mut delivery, session).await?;
                    let audit = actor.resource_log("delivery.post", "delivery", delivery_id.to_string())?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Delivery, crate::errors::Error>(delivery)
                })
            })
            .await?;
        Ok(posted.into())
    }
}

/// 过账单条仓发行（预占消耗 + 出库流水 + 余额，位于调用方事务内）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `delivery` - 发货单表头
/// * `line` - 发货行
/// * `occurred_at` - 过账业务时间
/// * `actor` - 审计操作人（记录人身份）
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 预占不存在/归属不符/数量不足、余额缺失或写入失败时返回错误。
async fn post_warehouse_ship_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
    delivery: &Delivery,
    line: &DeliveryLine,
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let reservation_id = line
        .stock_reservation_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("仓发必须消耗有效预占".to_string()))?;
    let reservation = db
        .stock_reservations()
        .find_by_id(reservation_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("库存预占不存在".to_string()))?;
    if reservation.sales_order_line_id != line.sales_order_line_id {
        return Err(Error::BusinessLogicError(
            "库存预占不属于本销售明细，不能消耗".to_string(),
        ));
    }
    if reservation.reserved_quantity.to_decimal() < line.quantity.to_decimal() {
        return Err(Error::BusinessLogicError(
            "为这单留的货不足，无法发货".to_string(),
        ));
    }
    let warehouse_id = delivery
        .warehouse_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("仓发缺少发货仓".to_string()))?;
    let balance = db
        .stock_balances()
        .find_by_dimensions(&warehouse_id, &reservation.sku_id, session)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("库存余额不存在，无法发货".to_string()))?;
    if !db
        .stock_reservations()
        .consume_quantity(&reservation.base.id, line.quantity, session)
        .await?
    {
        return Err(Error::BusinessLogicError(
            "预占数量不足或状态不符，无法消耗".to_string(),
        ));
    }
    let entry = StockReservationEntry::new(
        StockReservationEntryId::new(next_id()),
        StockReservationEntryData {
            reservation_id: reservation.base.id.clone().into(),
            entry_type: ReservationEntryType::Consume,
            quantity: line.quantity,
            source_document_id: delivery.base.id.clone(),
        },
    )?;
    db.stock_reservation_entries().create(&entry, session).await?;
    // 先释放预占再扣可用：预占建立时已扣减 available（reserved += q / available -= q），
    // 消耗本单预占发货时 available 已不含这部分，必须先释放（available += q）才能扣减
    if !db
        .stock_balances()
        .release_reserved(&balance.base.id, line.quantity, session)
        .await?
    {
        return Err(Error::BusinessLogicError("预占余额不足，无法发货".to_string()));
    }
    if !db
        .stock_balances()
        .deduct_available(&balance.base.id, line.quantity, session)
        .await?
    {
        return Err(Error::BusinessLogicError("可用库存不足，无法发货".to_string()));
    }
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id,
            sku_id: reservation.sku_id,
            movement_type: MovementType::WarehouseShipOut,
            direction: MovementDirection::Decrease,
            quantity: line.quantity,
            source_document_id: delivery.base.id.clone(),
            source_line_id: Some(line.base.id.clone()),
            reversal_of_movement_id: None,
            fact_no: next_id(),
            occurred_at: *occurred_at,
            recorded_at: *occurred_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )?;
    db.stock_movements().create(&movement, session).await?;
    // 余额记录最后流水（台账「最后变动」列），与数量增减同事务
    if !db
        .stock_balances()
        .apply_last_movement(&balance.base.id, &movement.base.id, session)
        .await?
    {
        return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
    }
    Ok(())
}
