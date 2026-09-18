use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::{
    PurchaseReceiptId, PurchaseReceiptLineId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionLineId,
    WarehouseId,
};
use erp_core::money::Quantity;
use erp_fulfillment::dto::{PostPurchaseReceiptRequest, PurchaseReceiptView};
use erp_fulfillment::entity::fulfillment::{PurchaseReceipt, PurchaseReceiptLine};
use erp_fulfillment::repository::FulfillmentExt;
use erp_inventory::repository::prelude::*;
use erp_inventory::{InventoryExt, StockReservation};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
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
        let expected_version = req.version;
        let warehouse_id = req.warehouse_id;
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let domain = erp_fulfillment::service::FulfillmentService::new(db.clone());
                    let (mut receipt, lines) = domain
                        .prepare_purchase_receipt_posting(
                            &receipt_id,
                            expected_version,
                            warehouse_id,
                            session,
                        )
                        .await?;
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
                    let mut received = db
                        .fulfillment()
                        .qualified_received_totals_by_purchase_revision_line(
                            &receipt.purchase_order_id,
                            session,
                        )
                        .await?;
                    let occurred_at = Instant::now();
                    execute_posting(
                        &mut MongoReceiptPosting {
                            db: &db,
                            receipt: &mut receipt,
                            po: &mut po,
                            lines: &lines,
                            revision_lines: &revision_lines,
                            received: &mut received,
                            occurred_at,
                            actor: &actor,
                            receipt_id: &receipt_id,
                        },
                        lines.len(),
                        session,
                    )
                    .await?;
                    Ok::<PurchaseReceipt, crate::Error>(receipt)
                })
            })
            .await?;
        Ok(posted.into())
    }
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
    async fn apply(&mut self, step: ReceiptPostingStep, executor: &mut dyn Executor) -> Result<()>;
}
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
    po: &'a mut erp_procurement::entity::purchase_order::PurchaseOrder,
    lines: &'a [PurchaseReceiptLine],
    revision_lines: &'a [erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine],
    received: &'a mut HashMap<erp_core::ids::PurchaseOrderRevisionLineId, Quantity>,
    occurred_at: Instant,
    actor: &'a AuditActor,
    receipt_id: &'a PurchaseReceiptId,
}
#[async_trait::async_trait]
impl ReceiptPostingSteps for MongoReceiptPosting<'_> {
    async fn apply(&mut self, step: ReceiptPostingStep, session: &mut dyn Executor) -> Result<()> {
        let db = self.db;
        let receipt = &mut *self.receipt;
        let po = &mut *self.po;
        let lines = self.lines;
        let revision_lines = self.revision_lines;
        let received = &mut *self.received;
        let occurred_at = self.occurred_at;
        let actor = self.actor;
        let receipt_id = self.receipt_id;
        match step {
            ReceiptPostingStep::Line(index) => {
                let line = &lines[index];
                let revision_line = revision_lines
                    .iter()
                    .find(|revision_line| {
                        revision_line.base.id == line.purchase_order_revision_line_id.to_string()
                    })
                    .ok_or_else(|| Error::BusinessLogicError("采购明细不存在".to_string()))?;
                let already_received = received
                    .get(&line.purchase_order_revision_line_id)
                    .copied()
                    .unwrap_or_else(|| Quantity::from_str("0").unwrap());
                line.ensure_within_revision(
                    &super::purchase_context::revision_line_fact(revision_line),
                    already_received,
                )
                .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                post_receipt_line(db, session, receipt, line, revision_lines, &occurred_at, actor).await?;
                match received.entry(line.purchase_order_revision_line_id.clone()) {
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
            },
            ReceiptPostingStep::Receipt => {
                erp_fulfillment::service::FulfillmentService::new(db.clone())
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
            ReceiptPostingStep::PurchaseProgress => {
                let revision_facts = revision_lines
                    .iter()
                    .map(super::purchase_context::revision_line_fact)
                    .collect::<Vec<_>>();
                let progress = match PurchaseReceipt::fulfillment_progress(&revision_facts, received) {
                    erp_fulfillment::entity::facts::ReceiptFulfillmentProgress::Partial => {
                        erp_procurement::entity::purchase_order::ProgressStatus::Partial
                    },
                    erp_fulfillment::entity::facts::ReceiptFulfillmentProgress::Completed => {
                        erp_procurement::entity::purchase_order::ProgressStatus::Completed
                    },
                };
                po.set_fulfillment_progress(progress, actor.id().to_string());
                db.purchase_orders().update(po, session).await?;
            },
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
    session: &mut dyn Executor,
    receipt: &PurchaseReceipt,
    line: &PurchaseReceiptLine,
    revision_lines: &[erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine],
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
    let balance_id = erp_inventory::service::fulfillment::post_receipt_stock(
        db,
        session,
        erp_inventory::service::fulfillment::ReceiptStockFact {
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
/// * `occurred_at` - 过账业务时间
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
    revision_line: &erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine,
    sku_id: &erp_core::ids::SkuId,
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
    let sales_revision_line_ids: Vec<SalesOrderRevisionLineId> =
        allocations.iter().map(|allocation| allocation.sales_order_revision_line_id.clone()).collect();
    let sales_revision_lines = db
        .sales_order_revision_lines()
        .list_active_by_ids(
            &sales_revision_line_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            session,
        )
        .await?;
    for (index, quantity) in shares.into_iter().enumerate() {
        let allocation = &allocations[index];
        let sales_line_id = sales_revision_lines
            .iter()
            .find(|sales_line| sales_line.base.id == allocation.sales_order_revision_line_id.to_string())
            .map(|sales_line| sales_line.sales_order_line_id.clone())
            .ok_or_else(|| Error::BusinessLogicError("采购销售分配缺少销售明细归属".to_string()))?;
        erp_inventory::service::fulfillment::establish_receipt_reservation(
            db,
            session,
            erp_inventory::service::fulfillment::ReceiptReservationFact {
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
mod tests {
    /// 过账路径不得启动审批、不得创建任务、不得选择定义。
    #[test]
    fn post_does_not_start_approval_or_create_tasks() {
        let production =
            include_str!("purchase_receipt_posting.rs").split("#[cfg(test)]").next().expect("生产代码");
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
        assert!(post.contains("mark_purchase_receipt_posted"));
        assert!(
            include_str!("../../../erp-fulfillment/src/service/purchase_receipt_posting.rs")
                .contains("receipt.mark_posted")
        );
        assert!(!post.contains("submit_"));
        assert!(!post.contains("start_approval"));
    }

    /// 累计有效收货必须由 Repository 聚合：旧 Service helper 已删除，过账
    /// 路径改调仓储聚合并继续在 Service 完成超收校验与进度更新。
    #[test]
    fn received_totals_are_aggregated_in_repository() {
        let production =
            include_str!("purchase_receipt_posting.rs").split("#[cfg(test)]").next().expect("生产代码");
        assert!(!production.contains("cumulative_received_quantities"), "旧 Service 聚合 helper 必须删除");
        assert!(!production.contains("list_posted_receipts_for_purchase_order"));
        assert!(
            production.contains("qualified_received_totals_by_purchase_revision_line"),
            "过账路径必须调用 Repository 聚合"
        );
        let post = production
            .split("pub async fn post_purchase_receipt")
            .nth(1)
            .and_then(|rest| rest.split("#[cfg(test)]").next())
            .expect("post_purchase_receipt 生产片段");
        assert!(post.contains("ensure_within_revision"), "超收校验保留在 Service");
        assert!(post.contains("fulfillment_progress"), "进度计算保留在 Service");
    }

    /// 入库预占必须按销售单与仓库复用草稿，并把新预占补成发货行。
    #[test]
    fn receipt_reservations_merge_into_exact_warehouse_draft() {
        let production =
            include_str!("purchase_receipt_posting.rs").split("#[cfg(test)]").next().expect("生产代码");
        let draft_flow =
            production.split("async fn create_warehouse_ship_drafts").nth(1).expect("仓发草稿流程");
        assert!(draft_flow.contains("by_order_warehouse"));
        assert!(draft_flow.contains("draft_warehouse_delivery"));
        assert!(draft_flow.contains("append_receipt_stock_delivery_lines"));
        assert!(!draft_flow.contains("draft_delivery_for_sales_order"));
    }
}

#[cfg(test)]
mod posting_contract_tests {
    use super::*;
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
