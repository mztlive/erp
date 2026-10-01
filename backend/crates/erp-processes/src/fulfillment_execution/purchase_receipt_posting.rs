use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::{
    PurchaseOrderRevisionLineId, PurchaseReceiptId, PurchaseReceiptLineId, SalesOrderId, SalesOrderLineId,
    SalesOrderRevisionLineId, SkuId, WarehouseId,
};
use erp_core::money::Quantity;
use erp_fulfillment::dto::{PostPurchaseReceiptRequest, PurchaseReceiptView};
use erp_fulfillment::entity::facts::ReceiptFulfillmentProgress;
use erp_fulfillment::entity::fulfillment::{PurchaseReceipt, PurchaseReceiptLine};
use erp_fulfillment::repository::FulfillmentExt;
use erp_fulfillment::service::FulfillmentService;
use erp_inventory::repository::prelude::*;
use erp_inventory::service::fulfillment::{
    ReceiptReservationFact, ReceiptStockFact, establish_receipt_reservation, post_receipt_stock,
};
use erp_inventory::{InventoryExt, StockReservation};
use erp_procurement::entity::purchase_order::{ProgressStatus, PurchaseOrder, PurchaseOrderRevisionLine};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::entity::sales_order::SalesOrderRevisionLine;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::FulfillmentProcess;
use super::purchase_context::{ensure_po_fulfillable, ensure_prepay_gate, load_po_current_revision};
use crate::{Error, Result};
impl FulfillmentProcess {
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
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_post",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "purchase_receipt_post")
    )]
    pub async fn post_purchase_receipt(
        &self,
        id: &str,
        req: PostPurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        req.validate()?;
        let receipt_id = PurchaseReceiptId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |executor| {
                Box::pin(async move { post_receipt(&db, &receipt_id, req, &actor, executor).await })
            })
            .await?;
        Ok(posted.into())
    }
}

/// 按原查询顺序准备入库事实，并在根事务内执行过账。
async fn post_receipt(
    db: &Database,
    receipt_id: &PurchaseReceiptId,
    req: PostPurchaseReceiptRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PurchaseReceipt> {
    let domain = FulfillmentService::new(db.clone());
    let (mut receipt, lines) =
        domain.prepare_purchase_receipt_posting(receipt_id, req.version, req.warehouse_id, executor).await?;
    let PurchasePostingFacts { mut po, revision_lines, mut received } =
        load_purchase_posting_facts(db, &receipt, executor).await?;
    let occurred_at = Instant::now();
    execute_posting(
        &mut MongoReceiptPosting {
            db,
            receipt: &mut receipt,
            po: &mut po,
            lines: &lines,
            revision_lines: &revision_lines,
            revision_lines_by_id: first_by_id(&revision_lines, |line| line.base.id.as_str()),
            received: &mut received,
            occurred_at,
            actor,
            receipt_id,
        },
        lines.len(),
        executor,
    )
    .await?;
    Ok(receipt)
}

/// 按原查询位置读取的采购入库事实，仅用于当前根事务。
struct PurchasePostingFacts {
    po: PurchaseOrder,
    revision_lines: Vec<PurchaseOrderRevisionLine>,
    received: HashMap<PurchaseOrderRevisionLineId, Quantity>,
}

/// 沿原事务执行器依次读取采购主表、履约门槛、有效版本行和累计收货。
async fn load_purchase_posting_facts(
    db: &Database,
    receipt: &PurchaseReceipt,
    executor: &mut dyn Executor,
) -> Result<PurchasePostingFacts> {
    let po = db
        .purchase_orders()
        .find_by_id(receipt.purchase_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
    ensure_po_fulfillable(&po)?;
    ensure_prepay_gate(db, executor, &po).await?;
    let revision = load_po_current_revision(db, executor, &po).await?;
    let revision_lines = db
        .purchase_order_revision_lines()
        .find_lines_by_revision_ids(&[revision.base.id.clone().into()], executor)
        .await?;
    let received = db
        .fulfillment()
        .qualified_received_totals_by_purchase_revision_line(&receipt.purchase_order_id, executor)
        .await?;
    Ok(PurchasePostingFacts { po, revision_lines, received })
}

/// 建立借用 ID 索引；重复 ID 保留原线性查找的首个命中，不改变输入顺序。
fn first_by_id<'a, T>(rows: &'a [T], id: impl Fn(&'a T) -> &'a str) -> HashMap<&'a str, &'a T> {
    let mut rows_by_id = HashMap::with_capacity(rows.len());
    for row in rows {
        rows_by_id.entry(id(row)).or_insert(row);
    }
    rows_by_id
}

/// 读取索引中的首个采购版本行，缺失时保留原业务错误。
fn purchase_line<'a>(
    lines_by_id: &HashMap<&str, &'a PurchaseOrderRevisionLine>,
    id: &PurchaseOrderRevisionLineId,
) -> Result<&'a PurchaseOrderRevisionLine> {
    lines_by_id
        .get(id.as_ref())
        .copied()
        .ok_or_else(|| Error::BusinessLogicError("采购明细不存在".to_string()))
}

/// 从首个销售版本行读取稳定明细身份，缺失时保留原分配归属错误。
fn sales_line_id(
    lines_by_id: &HashMap<&str, &SalesOrderRevisionLine>,
    id: &SalesOrderRevisionLineId,
) -> Result<SalesOrderLineId> {
    lines_by_id
        .get(id.as_ref())
        .map(|line| line.sales_order_line_id.clone())
        .ok_or_else(|| Error::BusinessLogicError("采购销售分配缺少销售明细归属".to_string()))
}

/// 采购入库生产过账步骤；每一步接收根事务的同一执行器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReceiptPostingStep {
    Line(usize),
    Receipt,
    Task,
    PurchaseProgress,
    WarehouseDrafts,
    Audit,
}
#[async_trait::async_trait]
trait ReceiptPostingSteps: Send {
    /// 沿根事务执行器执行指定步骤，失败时保留原错误。
    async fn apply(&mut self, step: ReceiptPostingStep, executor: &mut dyn Executor) -> Result<()>;
}
/// 按原入库行和后续领域步骤的顺序过账，任一步失败即停止。
async fn execute_posting(
    steps: &mut impl ReceiptPostingSteps,
    line_count: usize,
    executor: &mut dyn Executor,
) -> Result<()> {
    for index in 0..line_count {
        steps.apply(ReceiptPostingStep::Line(index), executor).await?;
    }
    for step in [
        ReceiptPostingStep::Receipt,
        ReceiptPostingStep::Task,
        ReceiptPostingStep::PurchaseProgress,
        ReceiptPostingStep::WarehouseDrafts,
        ReceiptPostingStep::Audit,
    ] {
        steps.apply(step, executor).await?;
    }
    Ok(())
}
struct MongoReceiptPosting<'a> {
    db: &'a Database,
    receipt: &'a mut PurchaseReceipt,
    po: &'a mut PurchaseOrder,
    lines: &'a [PurchaseReceiptLine],
    revision_lines: &'a [PurchaseOrderRevisionLine],
    revision_lines_by_id: HashMap<&'a str, &'a PurchaseOrderRevisionLine>,
    received: &'a mut HashMap<PurchaseOrderRevisionLineId, Quantity>,
    occurred_at: Instant,
    actor: &'a AuditActor,
    receipt_id: &'a PurchaseReceiptId,
}
#[async_trait::async_trait]
impl ReceiptPostingSteps for MongoReceiptPosting<'_> {
    /// 执行指定过账步骤；所有数据库操作继续复用调用方事务。
    async fn apply(&mut self, step: ReceiptPostingStep, session: &mut dyn Executor) -> Result<()> {
        let db = self.db;
        let receipt = &mut *self.receipt;
        let lines = self.lines;
        let occurred_at = self.occurred_at;
        let actor = self.actor;
        let receipt_id = self.receipt_id;
        match step {
            ReceiptPostingStep::Line(index) => self.post_line(index, session).await?,
            ReceiptPostingStep::Receipt => {
                FulfillmentService::new(db.clone())
                    .mark_purchase_receipt_posted(receipt, occurred_at, actor.id(), session)
                    .await?;
            },
            ReceiptPostingStep::Task => {
                super::task::complete_fulfillment_task(
                    db,
                    super::task::FulfillmentTaskObject::PurchaseReceipt(receipt),
                    actor.id(),
                    session,
                )
                .await?;
            },
            ReceiptPostingStep::PurchaseProgress => self.update_purchase_progress(session).await?,
            ReceiptPostingStep::WarehouseDrafts => {
                // 入库过账后自动生成仓发草稿与 W01 指定到人的仓发任务，
                // 行引用本次入库建立的销售预占。
                create_warehouse_ship_drafts(db, session, lines).await?;
            },
            ReceiptPostingStep::Audit => {
                let audit = actor.clone().resource_log(
                    "purchase_receipt.post",
                    "purchase_receipt",
                    receipt_id.to_string(),
                )?;
                db.audit_logs().create(&audit, session).await?;
            },
        }
        Ok(())
    }
}

impl MongoReceiptPosting<'_> {
    /// 定位一次当前采购版本行，依次校验、过账并更新累计合格数量。
    async fn post_line(&mut self, index: usize, session: &mut dyn Executor) -> Result<()> {
        let line = &self.lines[index];
        let revision_line = purchase_line(&self.revision_lines_by_id, &line.purchase_order_revision_line_id)?;
        let already_received = self
            .received
            .get(&line.purchase_order_revision_line_id)
            .copied()
            .unwrap_or_else(|| Quantity::from_str("0").unwrap());
        line.ensure_within_revision(
            &super::purchase_context::revision_line_fact(revision_line),
            already_received,
        )
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        post_receipt_line(self.db, session, self.receipt, line, revision_line, &self.occurred_at, self.actor)
            .await?;
        match self.received.entry(line.purchase_order_revision_line_id.clone()) {
            Entry::Occupied(mut occupied) => {
                let next = occupied
                    .get()
                    .to_decimal()
                    .checked_add(line.qualified_quantity.to_decimal())
                    .ok_or_else(|| Error::BusinessLogicError("累计数量超出精度上限".to_string()))?;
                *occupied.get_mut() = Quantity::try_from(next).map_err(Error::Logic)?;
            },
            Entry::Vacant(vacant) => {
                vacant.insert(line.qualified_quantity);
            },
        }
        Ok(())
    }

    /// 从原版本行顺序计算采购履约进度，再沿原执行器更新主表。
    async fn update_purchase_progress(&mut self, session: &mut dyn Executor) -> Result<()> {
        let revision_facts =
            self.revision_lines.iter().map(super::purchase_context::revision_line_fact).collect::<Vec<_>>();
        let progress = match PurchaseReceipt::fulfillment_progress(&revision_facts, self.received) {
            ReceiptFulfillmentProgress::Partial => ProgressStatus::Partial,
            ReceiptFulfillmentProgress::Completed => ProgressStatus::Completed,
        };
        self.po.set_fulfillment_progress(progress, self.actor.id().to_string());
        self.db.purchase_orders().update(self.po, session).await?;
        Ok(())
    }
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
/// * `revision_line` - 已定位并校验的采购生效版本行
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
    session: &mut dyn Executor,
    receipt: &PurchaseReceipt,
    line: &PurchaseReceiptLine,
    revision_line: &PurchaseOrderRevisionLine,
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let qualified = line.qualified_quantity.to_decimal();
    if qualified <= Quantity::from_str("0").unwrap().to_decimal() {
        return Ok(());
    }
    let sku_id = revision_line
        .sku_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("物流费用行不能入库".to_string()))?;
    let balance_id = post_receipt_stock(
        db,
        session,
        ReceiptStockFact {
            warehouse_id: &receipt.warehouse_id,
            sku_id: &sku_id,
            quantity: line.qualified_quantity,
            receipt_id: &receipt.base.id,
            receipt_line_id: &line.base.id,
        },
        *occurred_at,
        actor.id(),
    )
    .await?;
    establish_reservations(db, session, receipt, line, revision_line, &sku_id, &balance_id).await
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
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 分配缺失/无销售归属、可用量不足或写入失败时返回错误。
async fn establish_reservations(
    db: &Database,
    session: &mut dyn Executor,
    receipt: &PurchaseReceipt,
    line: &PurchaseReceiptLine,
    revision_line: &PurchaseOrderRevisionLine,
    sku_id: &SkuId,
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
    let total =
        revision_line.quantity.ok_or_else(|| Error::BusinessLogicError("采购明细缺少数量".to_string()))?;
    let allocation_quantities: Vec<Quantity> =
        allocations.iter().map(|allocation| allocation.allocated_quantity).collect();
    let shares = line
        .reservation_shares(&allocation_quantities, total)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    let sales_revision_lines = db
        .sales_order_revision_lines()
        .list_active_by_ids(
            &allocations
                .iter()
                .map(|allocation| allocation.sales_order_revision_line_id.to_string())
                .collect::<Vec<_>>(),
            session,
        )
        .await?;
    let sales_lines_by_id = first_by_id(&sales_revision_lines, |line| line.base.id.as_str());
    for (index, quantity) in shares.into_iter().enumerate() {
        let allocation = &allocations[index];
        let sales_line_id = sales_line_id(&sales_lines_by_id, &allocation.sales_order_revision_line_id)?;
        establish_receipt_reservation(
            db,
            session,
            ReceiptReservationFact {
                warehouse_id: &receipt.warehouse_id,
                sku_id,
                sales_order_line_id: sales_line_id,
                allocation_id: allocation.base.id.clone().into(),
                receipt_line_id: line.base.id.clone().into(),
                receipt_id: &receipt.base.id,
                balance_id,
                quantity,
            },
        )
        .await?;
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
    session: &mut dyn Executor,
    receipt_lines: &[PurchaseReceiptLine],
) -> Result<()> {
    if receipt_lines.is_empty() {
        return Ok(());
    }
    let receipt_line_ids: Vec<PurchaseReceiptLineId> =
        receipt_lines.iter().map(|line| line.base.id.clone().into()).collect();
    let reservations =
        db.stock_reservations().list_stock_reservations_for_receipt_lines(&receipt_line_ids, session).await?;
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
        .sales_order_lines()
        .list_active_by_ids(&line_ids.iter().map(ToString::to_string).collect::<Vec<_>>(), session)
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
    session: &mut dyn Executor,
) -> Result<()> {
    let domain = erp_fulfillment::service::FulfillmentService::new(db.clone());
    let existing = domain.draft_warehouse_delivery(sales_order_id, warehouse_id, session).await?;
    if let Some(delivery) = existing {
        domain
            .append_receipt_stock_delivery_lines(&delivery, &reservation_line_facts(reservations), session)
            .await?;
        super::task::ensure_fulfillment_task(
            db,
            super::task::FulfillmentTaskObject::Delivery(&delivery),
            session,
        )
        .await?;
        return Ok(());
    }
    let delivery = domain
        .create_receipt_stock_delivery(
            sales_order_id,
            warehouse_id,
            &reservation_line_facts(reservations),
            session,
        )
        .await?;
    super::task::ensure_fulfillment_task(
        db,
        super::task::FulfillmentTaskObject::Delivery(&delivery),
        session,
    )
    .await?;
    Ok(())
}

/// 在原仓发构造位置按库存预占顺序投影消费事实。
fn reservation_line_facts(
    reservations: &[&StockReservation],
) -> Vec<erp_fulfillment::entity::facts::ReceiptReservationLineFact> {
    reservations
        .iter()
        .map(|reservation| erp_fulfillment::entity::facts::ReceiptReservationLineFact {
            reservation_id: reservation.base.id.clone().into(),
            sales_order_line_id: reservation.sales_order_line_id.clone(),
            reserved_quantity: reservation.reserved_quantity,
        })
        .collect()
}

// 入库预占到仓发行字段映射归实体批量工厂（FUL-E01）；旧 Service 编号 helper 已删除。

#[cfg(test)]
mod posting_contract_tests {
    use entity_core::BaseModel;
    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseOrderRevisionId, SalesOrderRevisionId, SkuRevisionId,
    };
    use erp_core::money::{Amount, Rate};
    use erp_procurement::entity::purchase_order::PurchaseLineType;
    use erp_sales::entity::sales_order::LineType;

    use super::*;

    /// 构造可区分首个与重复命中内容的真实采购版本行。
    fn purchase_revision(id: &str, quantity: &str) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            purchase_order_revision_id: PurchaseOrderRevisionId::new("purchase-revision"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("confirmation-line")),
            sku_id: Some(SkuId::new("sku")),
            sku_revision_id: Some(SkuRevisionId::new("sku-revision")),
            product_name_snapshot: Some("商品".into()),
            specification_snapshot: None,
            quantity: Some(quantity.parse().unwrap()),
            base_unit_code: Some("件".into()),
            unit_cost_gross: Some("1".parse().unwrap()),
            gross_amount: quantity.parse().unwrap(),
            net_amount: quantity.parse().unwrap(),
            tax_amount: Amount::zero(),
            input_tax_rate: Some("0".parse().unwrap()),
            expected_delivery_date: None,
            sales_order_line_id: Some(SalesOrderLineId::new("sales-line")),
            sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sales-revision-line")),
            allocated_quantity: Some(quantity.parse().unwrap()),
        }
    }

    /// 构造真实销售版本行，使用不同稳定行身份识别重复主键的首个命中。
    fn sales_revision(id: &str, stable_id: &str) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            sales_order_revision_id: SalesOrderRevisionId::new("sales-revision"),
            sales_order_line_id: SalesOrderLineId::new(stable_id),
            line_no: 1,
            line_type: LineType::GoodsService,
            gross_amount: Amount::zero(),
            net_amount: Amount::zero(),
            tax_amount: Amount::zero(),
            sales_tax_rate: Rate::from_str("0").unwrap(),
            item_name_snapshot: "商品".into(),
            spec_snapshot: None,
            unit_snapshot: None,
        }
    }

    /// 采购索引与原首命中查找一致，且不移动或克隆版本行。
    #[test]
    fn receipt_purchase_index_preserves_first_match_and_requested_order() {
        let rows = [purchase_revision("b", "2"), purchase_revision("a", "3"), purchase_revision("b", "99")];
        let index = first_by_id(&rows, |line| line.base.id.as_str());
        for id in ["a", "b", "a"] {
            let found = purchase_line(&index, &PurchaseOrderRevisionLineId::new(id)).unwrap();
            let original = rows.iter().find(|line| line.base.id == id).unwrap();
            assert!(std::ptr::eq(found, original));
        }
        assert_eq!(
            purchase_line(&index, &PurchaseOrderRevisionLineId::new("b")).unwrap().quantity,
            Some("2".parse().unwrap())
        );
        assert!(matches!(
            purchase_line(&index, &PurchaseOrderRevisionLineId::new("missing")),
            Err(Error::BusinessLogicError(message)) if message == "采购明细不存在"
        ));
    }

    /// 销售身份映射仍沿调用方分配顺序执行，并保留重复版本行的首个稳定身份。
    #[test]
    fn receipt_sales_index_preserves_allocation_order_first_match_and_error() {
        let rows = [
            sales_revision("a", "stable-a"),
            sales_revision("b", "stable-b"),
            sales_revision("b", "later-b"),
        ];
        let index = first_by_id(&rows, |line| line.base.id.as_str());
        let stable_ids = ["b", "a", "b"]
            .into_iter()
            .map(|id| sales_line_id(&index, &SalesOrderRevisionLineId::new(id)).unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(stable_ids, ["stable-b", "stable-a", "stable-b"]);
        assert!(matches!(
            sales_line_id(&index, &SalesOrderRevisionLineId::new("missing")),
            Err(Error::BusinessLogicError(message)) if message == "采购销售分配缺少销售明细归属"
        ));
    }

    /// 空索引保持两种缺失关联的既有业务错误。
    #[test]
    fn receipt_empty_indexes_keep_missing_reference_errors() {
        let purchase_index = first_by_id(&[], |line: &PurchaseOrderRevisionLine| line.base.id.as_str());
        let sales_index = first_by_id(&[], |line: &SalesOrderRevisionLine| line.base.id.as_str());
        assert!(matches!(
            purchase_line(&purchase_index, &PurchaseOrderRevisionLineId::new("missing")),
            Err(Error::BusinessLogicError(_))
        ));
        assert!(matches!(
            sales_line_id(&sales_index, &SalesOrderRevisionLineId::new("missing")),
            Err(Error::BusinessLogicError(_))
        ));
    }

    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct RecordingSteps {
        calls: Vec<ReceiptPostingStep>,
        executor: usize,
        fail_at: Option<ReceiptPostingStep>,
    }
    #[async_trait::async_trait]
    impl ReceiptPostingSteps for RecordingSteps {
        async fn apply(&mut self, step: ReceiptPostingStep, ex: &mut dyn Executor) -> Result<()> {
            assert_eq!(ex as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(step);
            if self.fail_at == Some(step) {
                return Err(Error::ConflictError("原采购入库过账冲突".into()));
            }
            Ok(())
        }
    }
    const ORDER: [ReceiptPostingStep; 8] = [
        ReceiptPostingStep::Line(0),
        ReceiptPostingStep::Line(1),
        ReceiptPostingStep::Line(2),
        ReceiptPostingStep::Receipt,
        ReceiptPostingStep::Task,
        ReceiptPostingStep::PurchaseProgress,
        ReceiptPostingStep::WarehouseDrafts,
        ReceiptPostingStep::Audit,
    ];
    #[tokio::test]
    async fn receipt_posting_preserves_inventory_receipt_task_purchase_drafts_audit_order() {
        let mut ex = TestExecutor { _identity: 1 };
        let mut steps =
            RecordingSteps { calls: vec![], executor: &mut ex as *mut TestExecutor as usize, fail_at: None };
        execute_posting(&mut steps, 3, &mut ex).await.unwrap();
        assert_eq!(steps.calls, ORDER);
    }
    #[tokio::test]
    async fn receipt_posting_failure_stops_later_domains_and_retains_original_error() {
        for (index, step) in ORDER.iter().enumerate() {
            let mut ex = TestExecutor { _identity: 1 };
            let mut steps = RecordingSteps {
                calls: vec![],
                executor: &mut ex as *mut TestExecutor as usize,
                fail_at: Some(*step),
            };
            assert!(
                matches!(execute_posting(&mut steps,3,&mut ex).await,Err(Error::ConflictError(message)) if message=="原采购入库过账冲突")
            );
            assert_eq!(steps.calls, ORDER[..=index]);
        }
    }
}
