use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, FulfillmentExt, InventoryExt, PurchaseOrderExt, SalesOrderExt, Transactional,
};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::fulfillment::{
    Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryState, DeliveryType,
    PurchaseReceipt, PurchaseReceiptLine, PurchaseReceiptState,
};
use entities::ids::{
    DeliveryId, DeliveryLineId, PurchaseOrderId, PurchaseReceiptId, SalesOrderLineId,
    SalesOrderId, SalesOrderRevisionLineId, StockBalanceId, StockMovementId,
    StockReservationEntryId, StockReservationId,
};
use entities::inventory::{
    MovementDirection, MovementType, ReservationEntryType, ReservationStatus, StockBalance, StockBalanceData,
    StockMovement, StockMovementData, StockReservation, StockReservationData, StockReservationEntry,
    StockReservationEntryData,
};
use entities::money::Quantity;
use entities::purchase_order::ProgressStatus;
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::purchase_context::{ensure_po_fulfillable, ensure_prepay_gate, load_po_current_revision};
use super::{FulfillmentService, PurchaseReceiptView};

impl FulfillmentService {
    /// 过账采购入库（草稿 → 已过账；§8.2 第 1 条跨集合事务）。
    ///
    /// 在同一事务内：校验采购单可履约与 `PREPAY` 门槛（§8.1.5）、校验累计
    /// 有效收货不超当前有效采购数量、写入库行对应的库存增加流水、更新/创建
    /// 库存余额、沿采购销售分配自动建立销售预占（含预占流水）、推进采购履约
    /// 进度、迁移入库单状态、写审计。重复过账由状态守卫（仅草稿）与流水唯一
    /// 索引双重防护。
    ///
    /// # 参数
    /// * `id` - 入库单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的入库单视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单/采购单/生效版本不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `BusinessLogicError` - 门槛未满足、超收或采购单不可履约
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn post_purchase_receipt(&self, id: &str, actor: &AuditActor) -> Result<PurchaseReceiptView> {
        let receipt_id = PurchaseReceiptId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut receipt = db
                        .purchase_receipts()
                        .find_by_id(receipt_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
                    if receipt.status != PurchaseReceiptState::Draft {
                        return Err(Error::ConflictError(
                            "只有草稿状态的采购入库单可以过账".to_string(),
                        ));
                    }
                    let lines = db
                        .fulfillment()
                        .receipt_lines_by_receipt_ids(std::slice::from_ref(&receipt_id), session)
                        .await?;
                    if lines.is_empty() {
                        return Err(Error::ValidationError("采购入库单没有行，无法过账".to_string()));
                    }
                    let mut po = db
                        .purchase_orders()
                        .find_by_id(receipt.purchase_order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                    ensure_po_fulfillable(&po)?;
                    ensure_prepay_gate(&db, session, &po).await?;
                    let revision = load_po_current_revision(&db, session, &po).await?;
                    let revision_lines = db
                        .purchase_order_revision_lines()
                        .find_lines_by_revision_ids(&[revision.base.id.clone().into()], session)
                        .await?;
                    let mut received =
                        cumulative_received_quantities(&db, session, &receipt.purchase_order_id).await?;
                    let occurred_at = Instant::now();
                    for line in &lines {
                        ensure_receipt_within_revision(line, &revision_lines, &received)?;
                        post_receipt_line(
                            &db,
                            session,
                            &receipt,
                            line,
                            &revision_lines,
                            &occurred_at,
                            &actor,
                        )
                        .await?;
                        received
                            .entry(line.purchase_order_revision_line_id.to_string())
                            .and_modify(|total| {
                                *total = Quantity::try_from(
                                    total.to_decimal() + line.qualified_quantity.to_decimal(),
                                )
                                .expect("Quantity 加法和不超过精度上限")
                            })
                            .or_insert(line.qualified_quantity);
                    }
                    receipt.mark_posted(occurred_at, actor.id().to_string())?;
                    db.purchase_receipts().update(&mut receipt, session).await?;
                    let progress = compute_po_fulfillment_progress(&revision_lines, &received);
                    po.set_fulfillment_progress(progress, actor.id().to_string());
                    db.purchase_orders().update(&mut po, session).await?;
                    // 入库过账后自动生成仓发草稿：W09 仓储队列按 DRAFT 发货单
                    // 投影「公司仓发」待办，行引用本次入库建立的销售预占。
                    create_warehouse_ship_drafts(&db, session, &lines).await?;
                    let audit = actor.resource_log(
                        "purchase_receipt.post",
                        "purchase_receipt",
                        receipt_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PurchaseReceipt, crate::errors::Error>(receipt)
                })
            })
            .await?;
        Ok(posted.into())
    }
}

/// 统计采购单已过账入库的累计有效收货（按采购版本行分组）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po_id` - 采购单
///
/// # 返回
/// 返回「采购版本行 → 累计合格数量」映射。
///
/// # 错误
/// 任一步查询失败时返回 `RepositoryError`。
async fn cumulative_received_quantities(
    db: &Database,
    session: &mut mongodb::ClientSession,
    po_id: &PurchaseOrderId,
) -> Result<HashMap<String, Quantity>> {
    let receipts = db
        .purchase_receipts()
        .find_many(
            doc! {
                "purchase_order_id": po_id.to_string(),
                "status": PurchaseReceiptState::Posted.as_str(),
            },
            session,
        )
        .await?;
    let receipt_ids: Vec<PurchaseReceiptId> = receipts
        .iter()
        .map(|receipt| receipt.base.id.clone().into())
        .collect();
    let lines = db
        .fulfillment()
        .receipt_lines_by_receipt_ids(&receipt_ids, session)
        .await?;
    let mut totals: HashMap<String, Quantity> = HashMap::new();
    for line in lines {
        totals
            .entry(line.purchase_order_revision_line_id.to_string())
            .and_modify(|total| {
                *total = Quantity::try_from(total.to_decimal() + line.qualified_quantity.to_decimal())
                    .expect("Quantity 加法和不超过精度上限")
            })
            .or_insert(line.qualified_quantity);
    }
    Ok(totals)
}

/// 校验入库行累计有效收货不超过当前有效采购数量（§6.7）。
///
/// # 参数
/// * `line` - 入库行
/// * `revision_lines` - 采购生效版本行
/// * `received` - 已过账累计合格数量（不含本单）
///
/// # 返回
/// 不超收返回 `Ok(())`。
///
/// # 错误
/// 采购明细缺失、物流费用行不可入库或超收时返回 `ValidationError`/
/// `BusinessLogicError`。
fn ensure_receipt_within_revision(
    line: &PurchaseReceiptLine,
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    received: &HashMap<String, Quantity>,
) -> Result<()> {
    let revision_line = revision_lines
        .iter()
        .find(|revision_line| revision_line.base.id == line.purchase_order_revision_line_id.to_string())
        .ok_or_else(|| Error::BusinessLogicError("采购明细不存在".to_string()))?;
    let available = revision_line
        .quantity
        .ok_or_else(|| Error::BusinessLogicError("物流费用行不能入库".to_string()))?;
    let already = received
        .get(line.purchase_order_revision_line_id.as_ref())
        .copied()
        .unwrap_or_else(|| Quantity::from_str("0").unwrap());
    let posting =
        Quantity::try_from(line.qualified_quantity.to_decimal() + line.rejected_quantity.to_decimal())
            .map_err(Error::Logic)?;
    if already.to_decimal() + posting.to_decimal() > available.to_decimal() {
        return Err(Error::BusinessLogicError(
            "累计有效收货超过当前有效采购数量，超收必须走明确审批和采购变更".to_string(),
        ));
    }
    Ok(())
}

/// 过账单条入库行（流水 + 余额 + 预占，位于调用方事务内）。
///
/// 仅合格数量形成库存入账和销售预占（§6.7）；预占沿采购销售分配按比例
/// 分摊回原销售明细，最后一个分配吸收舍入尾差。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `receipt` - 入库单表头
/// * `line` - 入库行
/// * `revision_lines` - 采购生效版本行
/// * `occurred_at` - 过账业务时间
/// * `actor` - 审计操作人（记录人身份）
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 采购明细缺失、余额写入失败或预占建立失败时返回错误。
async fn post_receipt_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
    receipt: &PurchaseReceipt,
    line: &PurchaseReceiptLine,
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let qualified = line.qualified_quantity.to_decimal();
    if qualified <= Quantity::from_str("0").unwrap().to_decimal() {
        return Ok(());
    }
    let revision_line = revision_lines
        .iter()
        .find(|revision_line| revision_line.base.id == line.purchase_order_revision_line_id.to_string())
        .ok_or_else(|| Error::BusinessLogicError("采购明细不存在".to_string()))?;
    let sku_id = revision_line
        .sku_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("物流费用行不能入库".to_string()))?;
    let balance_id = ensure_or_create_balance(
        db,
        session,
        &receipt.warehouse_id,
        &sku_id,
        line.qualified_quantity,
    )
    .await?;
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id: receipt.warehouse_id.clone(),
            sku_id: sku_id.clone(),
            movement_type: MovementType::PurchaseReceiptIn,
            direction: MovementDirection::Increase,
            quantity: line.qualified_quantity,
            source_document_id: receipt.base.id.clone(),
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
        .apply_last_movement(&balance_id, &movement.base.id, session)
        .await?
    {
        return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
    }
    establish_reservations(db, session, receipt, line, revision_line, &sku_id, &balance_id).await
}

/// 建立/更新库存余额并返回余额主键（位于调用方事务内）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `warehouse_id` - 仓库
/// * `sku_id` - SKU
/// * `quantity` - 本次入库数量
///
/// # 返回
/// 返回余额主键。
///
/// # 错误
/// 余额写入失败时返回错误。
async fn ensure_or_create_balance(
    db: &Database,
    session: &mut mongodb::ClientSession,
    warehouse_id: &entities::ids::WarehouseId,
    sku_id: &entities::ids::SkuId,
    quantity: entities::money::Quantity,
) -> Result<String> {
    if let Some(balance) = db
        .stock_balances()
        .find_by_dimensions(warehouse_id, sku_id, session)
        .await?
    {
        if !db
            .stock_balances()
            .increase_on_hand(&balance.base.id, quantity, session)
            .await?
        {
            return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
        }
        return Ok(balance.base.id);
    }
    let balance = StockBalance::new(
        StockBalanceId::new(next_id()),
        StockBalanceData {
            warehouse_id: warehouse_id.clone(),
            sku_id: sku_id.clone(),
            on_hand_quantity: quantity,
            reserved_quantity: Quantity::from_str("0").map_err(Error::Logic)?,
            available_quantity: quantity,
            last_movement_id: None,
        },
    )?;
    db.stock_balances().create(&balance, session).await?;
    Ok(balance.base.id)
}

/// 沿采购销售分配自动建立销售预占（§8.2 第 1 条，位于调用方事务内）。
///
/// 预占数量按「分配数量 / 采购行数量」比例分摊本次合格入库，最后一个分配
/// 吸收舍入尾差；每个（入库行, 分配）的建立动作唯一由唯一索引保证。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `receipt` - 入库单表头
/// * `line` - 入库行
/// * `revision_line` - 采购生效版本行
/// * `sku_id` - SKU
/// * `balance_id` - 余额主键
/// * `occurred_at` - 过账业务时间
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 分配缺失/无销售归属、可用量不足或写入失败时返回错误。
async fn establish_reservations(
    db: &Database,
    session: &mut mongodb::ClientSession,
    receipt: &PurchaseReceipt,
    line: &PurchaseReceiptLine,
    revision_line: &entities::purchase_order::PurchaseOrderRevisionLine,
    sku_id: &entities::ids::SkuId,
    balance_id: &str,
) -> Result<()> {
    let allocations = db
        .purchase_line_sales_allocations()
        .find_by_purchase_revision_line_ids(
            std::slice::from_ref(&line.purchase_order_revision_line_id),
            session,
        )
        .await?;
    if allocations.is_empty() {
        return Ok(());
    }
    let total = revision_line
        .quantity
        .ok_or_else(|| Error::BusinessLogicError("采购明细缺少数量".to_string()))?;
    let allocation_quantities: Vec<Quantity> = allocations
        .iter()
        .map(|allocation| allocation.allocated_quantity)
        .collect();
    let shares = reservation_shares(line.qualified_quantity, &allocation_quantities, total)?;
    let sales_revision_line_ids: Vec<SalesOrderRevisionLineId> = allocations
        .iter()
        .map(|allocation| allocation.sales_order_revision_line_id.clone())
        .collect();
    let sales_revision_line_ids_str: Vec<String> =
        sales_revision_line_ids.iter().map(|id| id.to_string()).collect();
    let sales_revision_lines = db
        .sales_order_revision_lines()
        .find_many(doc! { "id": { "$in": sales_revision_line_ids_str } }, session)
        .await?;
    for (index, quantity) in shares.into_iter().enumerate() {
        let allocation = &allocations[index];
        let sales_line_id = sales_revision_lines
            .iter()
            .find(|sales_line| sales_line.base.id == allocation.sales_order_revision_line_id.to_string())
            .map(|sales_line| sales_line.sales_order_line_id.clone())
            .ok_or_else(|| Error::BusinessLogicError("采购销售分配缺少销售明细归属".to_string()))?;
        let reservation = StockReservation::new(
            StockReservationId::new(next_id()),
            StockReservationData {
                warehouse_id: receipt.warehouse_id.clone(),
                sku_id: sku_id.clone(),
                sales_order_line_id: sales_line_id,
                purchase_line_sales_allocation_id: allocation.base.id.clone().into(),
                source_receipt_line_id: line.base.id.clone().into(),
                reserved_quantity: quantity,
                consumed_quantity: Quantity::from_str("0").map_err(Error::Logic)?,
                released_quantity: Quantity::from_str("0").map_err(Error::Logic)?,
                status: ReservationStatus::Active,
            },
        )?;
        db.stock_reservations().create(&reservation, session).await?;
        if !db
            .stock_balances()
            .reserve_quantity(balance_id, quantity, session)
            .await?
        {
            return Err(Error::BusinessLogicError(
                "可用库存不足，无法建立销售预占".to_string(),
            ));
        }
        let entry = StockReservationEntry::new(
            StockReservationEntryId::new(next_id()),
            StockReservationEntryData {
                reservation_id: reservation.base.id.clone().into(),
                entry_type: ReservationEntryType::Establish,
                quantity,
                source_document_id: receipt.base.id.clone(),
            },
        )?;
        db.stock_reservation_entries().create(&entry, session).await?;
    }
    Ok(())
}

/// 按分配比例分摊本次合格入库数量（最后一个分配吸收舍入尾差）。
///
/// # 参数
/// * `qualified` - 本次合格入库数量
/// * `allocation_quantities` - 各采购销售分配的分配数量（按序分摊）
/// * `total` - 采购行总数
///
/// # 返回
/// 返回各分配的预占数量列表（与入参一一对应），合计恒等于 `qualified`。
///
/// # 错误
/// 采购行总数为 0 时返回 `BusinessLogicError`。
fn reservation_shares(
    qualified: Quantity,
    allocation_quantities: &[Quantity],
    total: Quantity,
) -> Result<Vec<Quantity>> {
    let zero = Quantity::from_str("0").unwrap();
    let total_dec = total.to_decimal();
    if total_dec <= zero.to_decimal() {
        return Err(Error::BusinessLogicError("采购明细数量必须为正数".to_string()));
    }
    let qualified_dec = qualified.to_decimal();
    let mut assigned = zero.to_decimal();
    let mut shares = Vec::with_capacity(allocation_quantities.len());
    for (index, allocation_quantity) in allocation_quantities.iter().enumerate() {
        let share = if index + 1 == allocation_quantities.len() {
            qualified_dec - assigned
        } else {
            (qualified_dec * allocation_quantity.to_decimal() / total_dec).round_dp(6)
        };
        assigned += share;
        shares.push(Quantity::try_from(share).map_err(|error| Error::BusinessLogicError(error.to_string()))?);
    }
    Ok(shares)
}

/// 按生效版本行计算采购履约进度（全部收满为已完成，否则部分执行）。
///
/// # 参数
/// * `revision_lines` - 采购生效版本行
/// * `received` - 累计有效收货（按采购版本行分组，含本次）
///
/// # 返回
/// 返回履约进度。
fn compute_po_fulfillment_progress(
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    received: &HashMap<String, Quantity>,
) -> ProgressStatus {
    let zero = Quantity::from_str("0").unwrap();
    let mut total = zero.to_decimal();
    for line in revision_lines {
        if let Some(quantity) = line.quantity {
            total += quantity.to_decimal();
        }
    }
    let mut received_total = zero.to_decimal();
    for line in revision_lines {
        if let Some(quantity) = received.get(&line.base.id) {
            received_total += quantity.to_decimal();
        }
    }
    if total > zero.to_decimal() && received_total >= total {
        ProgressStatus::Completed
    } else {
        ProgressStatus::Partial
    }
}

/// 入库过账后为涉及的销售单自动创建仓发草稿（幂等：已有 DRAFT 草稿则跳过）。
///
/// 仓发草稿行引用本次入库沿采购销售分配建立的预占：`delivery_line` 的
/// `stock_reservation_id` 指向预占，数量取预占数量；销售单已存在草稿时不
/// 重复创建（同一采购单多次入库只补新预占行由后续批量创建补充）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `receipt_lines` - 本次过账的入库行
///
/// # 返回
/// 无返回值；查询或写入失败时返回错误。
async fn create_warehouse_ship_drafts(
    db: &Database,
    session: &mut mongodb::ClientSession,
    receipt_lines: &[PurchaseReceiptLine],
) -> Result<()> {
    if receipt_lines.is_empty() {
        return Ok(());
    }
    let receipt_line_ids: Vec<String> = receipt_lines
        .iter()
        .map(|line| line.base.id.clone())
        .collect();
    let reservations = db
        .stock_reservations()
        .find_many(doc! { "source_receipt_line_id": { "$in": receipt_line_ids } }, session)
        .await?;
    if reservations.is_empty() {
        return Ok(());
    }
    // 按销售明细分组（一个销售明细可能对应多条预占/多次入库）
    let mut by_line: HashMap<String, Vec<&StockReservation>> = HashMap::new();
    for reservation in &reservations {
        by_line
            .entry(reservation.sales_order_line_id.to_string())
            .or_default()
            .push(reservation);
    }
    let line_ids: Vec<String> = by_line.keys().cloned().collect();
    let sales_lines = db
        .sales_order_lines()
        .find_many(doc! { "id": { "$in": line_ids } }, session)
        .await?;
    // 按销售单分组
    let mut by_order: HashMap<String, Vec<(&String, Vec<&StockReservation>)>> = HashMap::new();
    for (line_id, reservations) in &by_line {
        let sales_line = sales_lines
            .iter()
            .find(|line| line.base.id == **line_id)
            .ok_or_else(|| Error::BusinessLogicError("销售明细不存在，无法生成仓发草稿".to_string()))?;
        by_order
            .entry(sales_line.sales_order_id.to_string())
            .or_default()
            .push((line_id, reservations.clone()));
    }
    for (order_id, line_reservations) in by_order {
        let existing = db
            .deliveries()
            .find_one(
                doc! {
                    "sales_order_id": &order_id,
                    "status": DeliveryState::Draft.as_str(),
                },
                session,
            )
            .await?;
        if existing.is_some() {
            continue;
        }
        let delivery_id = DeliveryId::new(next_id());
        let warehouse_id = line_reservations
            .first()
            .and_then(|(_, reservations)| reservations.first())
            .map(|reservation| reservation.warehouse_id.clone())
            .ok_or_else(|| Error::BusinessLogicError("仓发草稿缺少仓库".to_string()))?;
        let delivery = Delivery::new(
            delivery_id.clone(),
            DeliveryData {
                delivery_no: format!("FH-{}", delivery_id.as_ref()),
                delivery_type: DeliveryType::WarehouseShip,
                sales_order_id: SalesOrderId::new(order_id.clone()),
                purchase_order_id: None,
                warehouse_id: Some(warehouse_id),
                carrier: None,
                tracking_no: None,
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )?;
        let mut lines = Vec::new();
        let mut line_no = 1u32;
        for (line_id, reservations) in &line_reservations {
            for reservation in reservations {
                lines.push(
                    DeliveryLine::new(
                        DeliveryLineId::new(next_id()),
                        DeliveryLineData {
                            delivery_id: delivery_id.clone(),
                            line_no,
                            sales_order_line_id: SalesOrderLineId::new((*line_id).clone()),
                            quantity: reservation.reserved_quantity,
                            stock_reservation_id: Some(reservation.base.id.clone().into()),
                            purchase_line_sales_allocation_id: None,
                        },
                        DeliveryType::WarehouseShip,
                    )
                    .map_err(Error::Logic)?,
                );
                line_no += 1;
            }
        }
        db.fulfillment()
            .create_delivery_with_lines(&delivery, &lines, session)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{compute_po_fulfillment_progress, reservation_shares};
    use entities::ids::{PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, SkuId};
    use entities::money::Quantity;
    use entities::purchase_order::ProgressStatus;
    use std::collections::HashMap;
    use std::str::FromStr;
    #[test]
    fn reservation_shares_split_proportionally_and_absorb_rounding() {
        let allocation_quantities = vec![
            Quantity::from_str("3").unwrap(),
            Quantity::from_str("2").unwrap(),
            Quantity::from_str("5").unwrap(),
        ];
        let shares = reservation_shares(
            Quantity::from_str("7.5").unwrap(),
            &allocation_quantities,
            Quantity::from_str("10").unwrap(),
        )
        .unwrap();
        let total: Quantity = shares
            .iter()
            .fold(Quantity::from_str("0").unwrap(), |acc, quantity| {
                Quantity::try_from(acc.to_decimal() + quantity.to_decimal()).unwrap()
            });
        assert_eq!(total, Quantity::from_str("7.5").unwrap());
        assert_eq!(shares[0], Quantity::from_str("2.25").unwrap());
        assert_eq!(shares[1], Quantity::from_str("1.5").unwrap());
        assert_eq!(shares[2], Quantity::from_str("3.75").unwrap());
    }

    #[test]
    fn po_fulfillment_progress_tracks_completion() {
        let mut received = HashMap::new();
        received.insert("porl-1".to_string(), Quantity::from_str("6").unwrap());
        assert_eq!(
            compute_po_fulfillment_progress(&[], &received),
            ProgressStatus::Partial,
            "无商品行且无累计收货视为部分执行"
        );
        received.insert("rev-line-1".to_string(), Quantity::from_str("10").unwrap());
        let lines = vec![entities::purchase_order::PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new("rev-line-1"),
            entities::purchase_order::PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new("rev-1"),
                line_no: 1,
                line_type: entities::purchase_order::PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(entities::ids::ProcurementConfirmationLineId::new(
                    "pcl-1",
                )),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: None,
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(Quantity::from_str("10").unwrap()),
                base_unit_code: Some("PCS".to_string()),
                unit_cost_gross: Some(entities::money::UnitPrice::from_str("10.0000").unwrap()),
                gross_amount: entities::money::Amount::from_str("100.00").unwrap(),
                net_amount: entities::money::Amount::from_str("87.00").unwrap(),
                tax_amount: entities::money::Amount::from_str("13.00").unwrap(),
                input_tax_rate: Some(entities::money::Rate::from_str("0.130000").unwrap()),
                expected_delivery_date: None,
            },
        )
        .unwrap()];
        assert_eq!(
            compute_po_fulfillment_progress(&lines, &received),
            ProgressStatus::Completed,
            "全部收满视为已完成"
        );
    }

    /// 过账路径不得启动审批、不得创建任务、不得选择定义。
    #[test]
    fn post_does_not_start_approval_or_create_tasks() {
        let production = include_str!("purchase_receipt_posting.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("pub async fn post_purchase_receipt"));
        assert!(!production.contains("start_approval"));
        assert!(!production.contains("prepare_start"));
        assert!(!production.contains("WorkItem"));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("PurchaseReceiptAdapter"));
        assert!(!production.contains("bind_published_definition_on_document_create"));
        let post = production
            .split("pub async fn post_purchase_receipt")
            .nth(1)
            .and_then(|rest| rest.split("#[cfg(test)]").next())
            .expect("post_purchase_receipt 生产片段");
        assert!(post.contains("mark_posted"));
        assert!(!post.contains("submit_"));
        assert!(!post.contains("start_approval"));
    }
}
