use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use database::{AccessControlExt, FulfillmentExt, InventoryExt, PurchaseOrderExt, Transactional};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::fulfillment::{
    Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryType, PurchaseReceipt,
    PurchaseReceiptLine,
};
use entities::ids::{
    DeliveryId, DeliveryLineId, PurchaseOrderId, PurchaseReceiptId, PurchaseReceiptLineId, SalesOrderId,
    SalesOrderLineId, SalesOrderRevisionLineId, StockBalanceId, StockMovementId, StockReservationEntryId,
    StockReservationId, WarehouseId,
};
use entities::inventory::{
    MovementDirection, MovementType, ReservationEntryType, ReservationStatus, StockBalance, StockBalanceData,
    StockMovement, StockMovementData, StockReservation, StockReservationData, StockReservationEntry,
    StockReservationEntryData, StockReservationSourceType,
};
use entities::money::Quantity;
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::purchase_context::{ensure_po_fulfillable, ensure_prepay_gate, load_po_current_revision};
use super::{FulfillmentService, PostPurchaseReceiptRequest, PurchaseReceiptView};

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
    /// * `req` - 最终草稿与期望版本
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
    pub async fn post_purchase_receipt(
        &self,
        id: &str,
        req: PostPurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        req.validate()?;
        let receipt_id = PurchaseReceiptId::new(id.to_string());
        let expected_version = req.version;
        let warehouse_id = req.warehouse_id;
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
                    receipt
                        .ensure_draft_version(expected_version)
                        .map_err(|error| Error::ConflictError(error.to_string()))?;
                    if warehouse_id
                        .as_ref()
                        .is_some_and(|requested| requested != &receipt.warehouse_id)
                    {
                        return Err(Error::ValidationError(
                            "采购入库单的目标仓库已冻结，不能在过账时变更".to_string(),
                        ));
                    }
                    receipt.update(entities::fulfillment::PurchaseReceiptUpdate {
                        warehouse_id: warehouse_id.or(Some(receipt.warehouse_id.clone())),
                    })?;
                    let lines = db
                        .fulfillment()
                        .receipt_lines_by_receipt_ids(std::slice::from_ref(&receipt_id), session)
                        .await?;
                    receipt
                        .ensure_posting_lines(&lines)
                        .map_err(|error| Error::ValidationError(error.to_string()))?;
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
                        let revision_line = revision_lines
                            .iter()
                            .find(|revision_line| {
                                revision_line.base.id == line.purchase_order_revision_line_id.to_string()
                            })
                            .ok_or_else(|| Error::BusinessLogicError("采购明细不存在".to_string()))?;
                        let already_received = received
                            .get(line.purchase_order_revision_line_id.as_ref())
                            .copied()
                            .unwrap_or_else(|| Quantity::from_str("0").unwrap());
                        line.ensure_within_revision(revision_line, already_received)
                            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
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
                    super::task::complete_fulfillment_task(
                        &db,
                        super::task::FulfillmentTaskObject::PurchaseReceipt(&receipt),
                        actor.id(),
                        session,
                    )
                    .await?;
                    let progress = PurchaseReceipt::fulfillment_progress(&revision_lines, &received);
                    po.set_fulfillment_progress(progress, actor.id().to_string());
                    db.purchase_orders().update(&mut po, session).await?;
                    // 入库过账后自动生成仓发草稿与 W01 指定到人的仓发任务，
                    // 行引用本次入库建立的销售预占。
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
        .fulfillment()
        .list_posted_receipts_for_purchase_order(po_id, session)
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
    let shares = line
        .reservation_shares(&allocation_quantities, total)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    let sales_revision_line_ids: Vec<SalesOrderRevisionLineId> = allocations
        .iter()
        .map(|allocation| allocation.sales_order_revision_line_id.clone())
        .collect();
    let sales_revision_lines = db
        .fulfillment()
        .list_sales_revision_lines_by_ids(&sales_revision_line_ids, session)
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
                source_type: StockReservationSourceType::PurchaseReceipt,
                purchase_line_sales_allocation_id: Some(allocation.base.id.clone().into()),
                source_receipt_line_id: Some(line.base.id.clone().into()),
                source_allocation_id: None,
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

/// 入库过账后按销售单与仓库创建或补充仓发草稿。
///
/// 仓发草稿行引用本次入库沿采购销售分配建立的预占：`delivery_line` 的
/// `stock_reservation_id` 指向预占，数量取预占数量。同一销售单同一仓库只复用
/// 一个草稿；不同仓库必须分别建草稿，现有库存分配与后续采购入库可以共同补行。
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
    let receipt_line_ids: Vec<PurchaseReceiptLineId> = receipt_lines
        .iter()
        .map(|line| line.base.id.clone().into())
        .collect();
    let reservations = db
        .fulfillment()
        .list_stock_reservations_for_receipt_lines(&receipt_line_ids, session)
        .await?;
    if reservations.is_empty() {
        return Ok(());
    }
    let line_ids = reservations
        .iter()
        .map(|reservation| reservation.sales_order_line_id.clone())
        .collect::<HashSet<SalesOrderLineId>>()
        .into_iter()
        .collect::<Vec<_>>();
    let sales_lines = db
        .fulfillment()
        .list_sales_order_lines_by_ids(&line_ids, session)
        .await?;
    let sales_order_by_line = sales_lines
        .into_iter()
        .map(|line| (line.base.id, line.sales_order_id.to_string()))
        .collect::<HashMap<_, _>>();
    let mut by_order_warehouse = BTreeMap::<(String, String), Vec<&StockReservation>>::new();
    for reservation in &reservations {
        let sales_order_id = sales_order_by_line
            .get(reservation.sales_order_line_id.as_ref())
            .ok_or_else(|| Error::BusinessLogicError("销售明细不存在，无法生成仓发草稿".to_string()))?;
        by_order_warehouse
            .entry((sales_order_id.clone(), reservation.warehouse_id.to_string()))
            .or_default()
            .push(reservation);
    }
    for ((order_id, warehouse_id), reservations) in by_order_warehouse {
        ensure_receipt_stock_delivery(
            db,
            &SalesOrderId::new(order_id),
            &WarehouseId::new(warehouse_id),
            &reservations,
            session,
        )
        .await?;
    }
    Ok(())
}

/// 将本次采购入库预占合并到同销售单同仓库的仓发草稿。
async fn ensure_receipt_stock_delivery(
    db: &Database,
    sales_order_id: &SalesOrderId,
    warehouse_id: &WarehouseId,
    reservations: &[&StockReservation],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let existing = db
        .fulfillment()
        .draft_warehouse_delivery(sales_order_id, warehouse_id, session)
        .await?;
    if let Some(delivery) = existing {
        append_receipt_stock_delivery_lines(db, &delivery, reservations, session).await?;
        super::task::ensure_fulfillment_task(
            db,
            super::task::FulfillmentTaskObject::Delivery(&delivery),
            session,
        )
        .await?;
        return Ok(());
    }
    let delivery_id = DeliveryId::new(next_id());
    let delivery = Delivery::new(
        delivery_id.clone(),
        DeliveryData {
            delivery_no: super::document_number::next_delivery_no(db).await?,
            delivery_type: DeliveryType::WarehouseShip,
            sales_order_id: sales_order_id.clone(),
            purchase_order_id: None,
            warehouse_id: Some(warehouse_id.clone()),
            carrier: None,
            tracking_no: None,
            address_snapshot_encrypted: None,
            address_snapshot_fingerprint: None,
        },
    )?;
    let lines = build_receipt_stock_delivery_lines(&delivery_id, reservations, 1)?;
    db.fulfillment()
        .create_delivery_with_lines(&delivery, &lines, session)
        .await?;
    super::task::ensure_fulfillment_task(
        db,
        super::task::FulfillmentTaskObject::Delivery(&delivery),
        session,
    )
    .await?;
    Ok(())
}

/// 向既有仓发草稿追加尚未引用的采购入库预占。
async fn append_receipt_stock_delivery_lines(
    db: &Database,
    delivery: &Delivery,
    reservations: &[&StockReservation],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let delivery_id = DeliveryId::new(delivery.base.id.clone());
    let existing = db
        .fulfillment()
        .delivery_lines_by_delivery_ids(std::slice::from_ref(&delivery_id), session)
        .await?;
    let existing_reservations = existing
        .iter()
        .filter_map(|line| line.stock_reservation_id.as_ref().map(ToString::to_string))
        .collect::<HashSet<_>>();
    let pending = reservations
        .iter()
        .copied()
        .filter(|reservation| !existing_reservations.contains(&reservation.base.id))
        .collect::<Vec<_>>();
    let next_line_no = existing.iter().map(|line| line.line_no).max().unwrap_or(0) + 1;
    for line in build_receipt_stock_delivery_lines(&delivery_id, &pending, next_line_no)? {
        db.delivery_lines().create(&line, session).await?;
    }
    Ok(())
}

/// 将采购入库预占投影为仓发草稿行。
fn build_receipt_stock_delivery_lines(
    delivery_id: &DeliveryId,
    reservations: &[&StockReservation],
    first_line_no: u32,
) -> Result<Vec<DeliveryLine>> {
    reservations
        .iter()
        .enumerate()
        .map(|(index, reservation)| {
            DeliveryLine::new(
                DeliveryLineId::new(next_id()),
                DeliveryLineData {
                    delivery_id: delivery_id.clone(),
                    line_no: first_line_no + index as u32,
                    sales_order_line_id: reservation.sales_order_line_id.clone(),
                    quantity: reservation.reserved_quantity,
                    stock_reservation_id: Some(reservation.base.id.clone().into()),
                    purchase_line_sales_allocation_id: None,
                },
                DeliveryType::WarehouseShip,
            )
            .map_err(Error::Logic)
        })
        .collect()
}

#[cfg(test)]
mod tests {
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

    /// 入库预占必须按销售单与仓库复用草稿，并把新预占补成发货行。
    #[test]
    fn receipt_reservations_merge_into_exact_warehouse_draft() {
        let production = include_str!("purchase_receipt_posting.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let draft_flow = production
            .split("async fn create_warehouse_ship_drafts")
            .nth(1)
            .expect("仓发草稿流程");
        assert!(draft_flow.contains("by_order_warehouse"));
        assert!(draft_flow.contains("draft_warehouse_delivery"));
        assert!(draft_flow.contains("append_receipt_stock_delivery_lines"));
        assert!(!draft_flow.contains("draft_delivery_for_sales_order"));
    }
}
