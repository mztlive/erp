//! 按供给分配结果一次落地现有库存预占和采购缺口。
//!
//! 操作人确认库存优先的推荐结果后，本模块在同一事务内推进一次供给 guard、
//! 原子预占现有库存并生成仓发草稿，再把剩余采购分配按供应商、采购类型、
//! 付款条件和履约责任拆成采购单并启动审批。

use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

use database::{
    AccessControlExt, Executor, FulfillmentExt, InventoryExt, NoTransaction, SalesOrderExt, WorkItemExt,
};
use entities::fulfillment::{Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryType};
use entities::ids::{
    DeliveryId, DeliveryLineId, SalesOrderId, SalesOrderLineId, StockReservationEntryId, StockReservationId,
    WarehouseId,
};
use entities::inventory::{
    ReservationEntryType, ReservationStatus, StockReservation, StockReservationData, StockReservationEntry,
    StockReservationEntryData, StockReservationSourceType,
};
use entities::money::Quantity;
use entities::purchase_order::{
    payload_fingerprint, LegacyReceiptIdScheme, PurchaseCommandReceipt, PurchaseCommandReceiptError,
};
use entities::work_item::{WorkItemStatus, WorkItemType};
use id_generator::next_id;
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::creation_basis::{
    basis_groups_and_facts, basis_groups_for_order, load_effective_sales_order, persist_basis_draft,
    procurement_quantity_changed, stock_basis_groups_for_order, validate_requested_quantities,
    CreateBasisCommand, VerifiedBasisInput,
};
use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest, CreatePurchaseOrderResult,
    CreatePurchaseOrdersFromSourcingRequest, CreatePurchaseOrdersFromSourcingResult,
    ExistingStockReservationResult, CREATE_SOURCING_ACTION,
};
use super::procurement_task_sync::{
    load_owned_open_procurement_task, sync_procurement_tasks_for_sales_order,
};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use entities::purchase_order::{
    basis_id_for, SourcingAssignmentSet, SourcingPlan, SourcingPlanError, StockAllocationPlan,
    StockBasisGroup,
};

const CREATE_PERMISSION: &str = "purchase_order:create";
const CREATE_SOURCING_RECEIPT_PREFIX: &str = "purchase-order-sourcing-command-";
const CREATE_SOURCING_ITEM_PREFIX: &str = "purchase-order-sourcing-item-";

/// 选源命令中单张已提交采购单的幂等收据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourcingOrderReceipt {
    /// 采购单主键。
    purchase_order_id: String,
    /// 采购单号。
    purchase_no: String,
    /// 创建完成时乐观锁版本。
    lock_version: u64,
}

/// 选源命令幂等收据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourcingReceipt {
    /// 本次创建并已提交审批的全部采购单。
    orders: Vec<SourcingOrderReceipt>,
    /// 本次建立的现有库存预占。
    #[serde(default)]
    stock_reservations: Vec<ExistingStockReservationResult>,
    /// 本次命令同步完成时的原任务状态。
    #[serde(default)]
    work_item_status: Option<WorkItemStatus>,
}

/// 已持久化的现有库存分配及其公开结果。
struct PersistedStockAllocation {
    /// 新建库存预占。
    reservation: StockReservation,
    /// API 返回投影。
    result: ExistingStockReservationResult,
}

impl PurchaseOrderService {
    /// 按供给分配行一次预占现有库存并创建采购缺口单。
    ///
    /// # 参数
    /// * `req` - 来源销售单、供给分配任务、逐行供给依据与数量、幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回本次库存预占和按精确拆分维度创建并已提交审批的采购单；同一幂等键与同一载荷重复提交时返回原结果。
    ///
    /// # 错误
    /// 操作账号不可登录或缺少采购创建权限、供给行重复或依据失效、数量非正或超过
    /// 事务内最新剩余/可供量、幂等键载荷冲突、并发冲突、审批绑定、启动审批或仓储写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 同一销售行可按库存与采购精确依据拆分；库存直接形成预占与仓发草稿，采购缺口按拆分维度建单；
    /// 操作人授权版本通过 policy CAS 与提交绑定，事务内只推进一次销售单供给 guard；
    /// 采购单创建成功即进入审批中，不得留下可编辑草稿。
    pub async fn create_from_sourcing(
        &self,
        req: CreatePurchaseOrdersFromSourcingRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrdersFromSourcingResult> {
        req.validate()?;
        let assignments = req.sourcing_assignments()?;
        let request_fingerprint = req.request_fingerprint(assignments.assignments())?;
        let sales_order_id = SalesOrderId::new(req.sales_order_id.trim().to_string());
        let receipt_identity = PurchaseCommandReceipt::<SourcingReceipt>::identity(
            CREATE_SOURCING_RECEIPT_PREFIX,
            actor.id(),
            CREATE_SOURCING_ACTION,
            Some(sales_order_id.as_ref()),
            &req.idempotency_key,
            LegacyReceiptIdScheme::None,
        )?;
        let audit_id = receipt_identity.receipt_id().to_string();
        let PurchaseOrderAuthorization {
            rbac,
            policy_revision,
        } = self.authorize_actor_permission(actor, CREATE_PERMISSION).await?;
        if let Some(result) = replay_sourcing(
            &self.db,
            &audit_id,
            &request_fingerprint,
            actor,
            sales_order_id.as_ref(),
            &req.work_item_id,
            &mut NoTransaction,
        )
        .await?
        {
            return Ok(result);
        }
        let db = self.db.clone();
        let binding_rbac = rbac.clone();
        let transaction_actor = actor.clone();
        let transaction_req = req.clone();
        let transaction_fingerprint = request_fingerprint.clone();
        let transaction_audit_id = audit_id.clone();
        let transaction_sales_order_id = sales_order_id.clone();
        let transaction_result = rbac
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    ensure_purchase_order_actor_account(&db, &transaction_actor, session).await?;
                    create_from_sourcing_in_transaction(
                        &db,
                        &binding_rbac,
                        &transaction_req,
                        &assignments,
                        &transaction_sales_order_id,
                        &transaction_audit_id,
                        &transaction_fingerprint,
                        &transaction_actor,
                        session,
                    )
                    .await
                })
            })
            .await;
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => replay_sourcing(
                &self.db,
                &audit_id,
                &request_fingerprint,
                actor,
                sales_order_id.as_ref(),
                &req.work_item_id,
                &mut NoTransaction,
            )
            .await?
            .ok_or(error),
        }
    }
}

/// 在 MongoDB 事务内按供给计划写入库存预占、仓发草稿和采购单。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `req` - 原始选源请求
/// * `assignments` - 已规范化且稳定行不重复的选源集合
/// * `sales_order_id` - 来源销售单
/// * `audit_id` - 整批命令收据 ID
/// * `request_fingerprint` - 整批命令载荷指纹
/// * `actor` - 审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回本次创建或事务内命中的幂等结果。
///
/// # 错误
/// 任务、依据、数量、并发 guard、审批绑定或持久化失败时返回错误。
///
/// # 关键业务约束
/// guard CAS 成功后必须再次按统一供给覆盖计算剩余量，且本函数只推进一次 guard。
#[allow(clippy::too_many_arguments)]
async fn create_from_sourcing_in_transaction(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    req: &CreatePurchaseOrdersFromSourcingRequest,
    assignments: &SourcingAssignmentSet,
    sales_order_id: &SalesOrderId,
    audit_id: &str,
    request_fingerprint: &str,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrdersFromSourcingResult> {
    if let Some(result) = replay_sourcing(
        db,
        audit_id,
        request_fingerprint,
        actor,
        sales_order_id.as_ref(),
        &req.work_item_id,
        session,
    )
    .await?
    {
        return Ok(result);
    }
    let task =
        load_owned_open_procurement_task(db, &req.work_item_id, sales_order_id, actor.id(), session).await?;
    let mut order = load_effective_sales_order(db, sales_order_id, session).await?;
    let groups = basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    let stock_groups =
        stock_basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    let plan = SourcingPlan::plan(&order, &groups, &stock_groups, &req.work_item_id, assignments)
        .map_err(map_sourcing_plan_error)?;
    order.advance_procurement_guard(actor.id())?;
    db.sales_orders().update(&mut order, session).await?;
    let latest_stock_groups =
        stock_basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    plan.validate_against_latest_stock(&latest_stock_groups)
        .map_err(map_sourcing_plan_error)?;
    let persisted_stock = persist_stock_allocations(
        db,
        plan.stock_plans(),
        &latest_stock_groups,
        sales_order_id,
        audit_id,
        request_fingerprint,
        session,
    )
    .await?;
    create_stock_delivery_drafts(db, sales_order_id, &persisted_stock, session).await?;
    let stock_reservations = persisted_stock
        .into_iter()
        .map(|allocation| allocation.result)
        .collect::<Vec<_>>();
    let (latest_groups, latest_facts) =
        basis_groups_and_facts(db, &order, task.responsibility_scope_ids(), session).await?;
    plan.validate_against_latest_sourcing(&latest_groups)
        .map_err(map_sourcing_plan_error)?;
    let mut orders = Vec::with_capacity(plan.purchase_plans().len());
    for plan in plan.purchase_plans() {
        let latest = latest_groups
            .iter()
            .find(|group| group.scope == plan.group.scope)
            .ok_or_else(procurement_quantity_changed)?;
        let selected_lines = validate_requested_quantities(&plan.requested_lines, latest)?;
        let basis_id = basis_id_for(
            &order,
            latest,
            &req.work_item_id,
            plan.target_warehouse_id.as_ref(),
        );
        let item_req = CreatePurchaseOrderFromBasisRequest {
            work_item_id: req.work_item_id.clone(),
            basis_id: basis_id.clone(),
            purchase_type: latest.scope.purchase_type,
            payment_term_code: latest.scope.payment_term_code.clone(),
            target_warehouse_id: plan.target_warehouse_id.as_ref().map(ToString::to_string),
            lines: plan
                .requested_lines
                .iter()
                .map(|line| CreatePurchaseOrderLineRequest {
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    quantity: line.quantity.to_string(),
                    expected_delivery_date: line.expected_delivery_date.to_string(),
                })
                .collect(),
            idempotency_key: req.idempotency_key.clone(),
        };
        let item_receipt_identity = PurchaseCommandReceipt::<SourcingReceipt>::identity(
            CREATE_SOURCING_ITEM_PREFIX,
            actor.id(),
            CREATE_SOURCING_ACTION,
            Some(basis_id.as_str()),
            &req.idempotency_key,
            LegacyReceiptIdScheme::None,
        )?;
        let item_audit_id = item_receipt_identity.receipt_id().to_string();
        let command = CreateBasisCommand {
            sales_order_id,
            req: &item_req,
            requested_lines: &plan.requested_lines,
            audit_id: &item_audit_id,
            request_fingerprint,
            actor,
        };
        orders.push(
            persist_basis_draft(
                db,
                rbac,
                &VerifiedBasisInput {
                    sales_order: &order,
                    group: latest,
                    selected_lines: &selected_lines,
                    facts: &latest_facts,
                },
                &command,
                session,
            )
            .await?,
        );
    }
    sync_procurement_tasks_for_sales_order(db, sales_order_id, session).await?;
    let work_item_status = db
        .work_items()
        .find_by_id(&req.work_item_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("供给分配任务在同步后不存在".to_string()))?
        .status;
    let response_work_item_status = sourcing_work_item_status(work_item_status, false)?;
    let receipt = SourcingReceipt {
        orders: orders
            .iter()
            .map(|order| SourcingOrderReceipt {
                purchase_order_id: order.purchase_order_id.clone(),
                purchase_no: order.purchase_no.clone(),
                lock_version: order.lock_version,
            })
            .collect(),
        stock_reservations: stock_reservations.clone(),
        work_item_status: Some(work_item_status),
    };
    write_sourcing_receipt(
        db,
        audit_id,
        request_fingerprint,
        sales_order_id.as_ref(),
        &receipt,
        actor,
        session,
    )
    .await?;
    Ok(CreatePurchaseOrdersFromSourcingResult {
        orders,
        stock_reservations,
        work_item_status: response_work_item_status,
        replayed: false,
        reference: sales_order_id.to_string(),
    })
}

/// 已规范化的选源行。
/// 查找 guard 后仍有效的库存余额依据。
///
/// # 参数
/// * `groups` - 最新库存余额依据
/// * `balance_id` - 计划命中的余额主键
///
/// # 返回
/// 命中时返回该余额依据。
///
/// # 错误
/// 余额已失效时返回可刷新冲突。
///
/// # 关键业务约束
/// 余额依据在 guard 推进后可能被作废释放，必须以最新集合查找。
fn latest_stock_group<'a>(groups: &'a [StockBasisGroup], balance_id: &str) -> Result<&'a StockBasisGroup> {
    groups
        .iter()
        .find(|group| group.balance.base.id == balance_id)
        .ok_or_else(procurement_quantity_changed)
}

/// 把选源计划领域错误映射为服务层稳定业务错误。
///
/// # 参数
/// * `error` - 选源计划领域错误
///
/// # 返回
/// 依据失效映射为可刷新冲突，仓库契约违规映射为参数验证错误。
///
/// # 错误
/// 无。
fn map_sourcing_plan_error(error: SourcingPlanError) -> Error {
    match error {
        SourcingPlanError::StaleFacts => procurement_quantity_changed(),
        SourcingPlanError::WarehouseContract(message) => Error::ValidationError(message),
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_stock_allocations(
    db: &mongodb::Database,
    plans: &[StockAllocationPlan],
    latest_groups: &[StockBasisGroup],
    sales_order_id: &SalesOrderId,
    audit_id: &str,
    request_fingerprint: &str,
    session: &mut ClientSession,
) -> Result<Vec<PersistedStockAllocation>> {
    let zero = Quantity::from_str("0").map_err(Error::Logic)?;
    let mut persisted = Vec::new();
    for plan in plans {
        let latest = latest_stock_group(latest_groups, &plan.group.balance.base.id)?;
        for requested in &plan.requested_lines {
            let line = latest
                .line_for(&requested.sales_order_line_id)
                .ok_or_else(procurement_quantity_changed)?;
            if !db
                .stock_balances()
                .reserve_quantity(&latest.balance.base.id, requested.quantity, session)
                .await?
            {
                return Err(procurement_quantity_changed());
            }
            let source_allocation_id = payload_fingerprint(
                "inventory.allocate_existing_stock",
                sales_order_id.as_ref(),
                &(
                    request_fingerprint,
                    latest.balance.base.id.as_str(),
                    requested.sales_order_line_id.as_str(),
                    requested.quantity.to_string(),
                ),
            )?;
            let reservation = StockReservation::new(
                StockReservationId::new(next_id()),
                StockReservationData {
                    warehouse_id: latest.balance.warehouse_id.clone(),
                    sku_id: line.coverage.goods_line.sku_id.clone(),
                    sales_order_line_id: SalesOrderLineId::new(requested.sales_order_line_id.clone()),
                    source_type: StockReservationSourceType::ExistingStock,
                    purchase_line_sales_allocation_id: None,
                    source_receipt_line_id: None,
                    source_allocation_id: Some(source_allocation_id),
                    reserved_quantity: requested.quantity,
                    consumed_quantity: zero,
                    released_quantity: zero,
                    status: ReservationStatus::Active,
                },
            )?;
            db.stock_reservations().create(&reservation, session).await?;
            let entry = StockReservationEntry::new(
                StockReservationEntryId::new(next_id()),
                StockReservationEntryData {
                    reservation_id: reservation.base.id.clone().into(),
                    entry_type: ReservationEntryType::Establish,
                    quantity: requested.quantity,
                    source_document_id: audit_id.to_string(),
                },
            )?;
            db.stock_reservation_entries().create(&entry, session).await?;
            persisted.push(PersistedStockAllocation {
                result: ExistingStockReservationResult {
                    stock_reservation_id: reservation.base.id.clone(),
                    sales_order_line_id: requested.sales_order_line_id.clone(),
                    stock_balance_id: latest.balance.base.id.clone(),
                    warehouse_id: latest.balance.warehouse_id.to_string(),
                    quantity: requested.quantity.to_string(),
                },
                reservation,
            });
        }
    }
    Ok(persisted)
}

/// 为现有库存预占创建或补充同仓仓发草稿。
async fn create_stock_delivery_drafts(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    allocations: &[PersistedStockAllocation],
    session: &mut ClientSession,
) -> Result<()> {
    let mut by_warehouse = BTreeMap::<String, Vec<&StockReservation>>::new();
    for allocation in allocations {
        by_warehouse
            .entry(allocation.reservation.warehouse_id.to_string())
            .or_default()
            .push(&allocation.reservation);
    }
    for (warehouse_id, reservations) in by_warehouse {
        ensure_stock_delivery_for_warehouse(
            db,
            sales_order_id,
            &WarehouseId::new(warehouse_id),
            &reservations,
            session,
        )
        .await?;
    }
    Ok(())
}

/// 创建一个仓发草稿，或向同销售单同仓草稿补入新的预占行。
async fn ensure_stock_delivery_for_warehouse(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    warehouse_id: &WarehouseId,
    reservations: &[&StockReservation],
    session: &mut ClientSession,
) -> Result<()> {
    let existing = db
        .fulfillment()
        .draft_warehouse_delivery(sales_order_id, warehouse_id, session)
        .await?;
    if let Some(delivery) = existing {
        append_stock_delivery_lines(db, &delivery, reservations, session).await?;
        crate::fulfillment::task::ensure_fulfillment_task(
            db,
            crate::fulfillment::task::FulfillmentTaskObject::Delivery(&delivery),
            session,
        )
        .await?;
        return Ok(());
    }
    let delivery_id = DeliveryId::new(next_id());
    let delivery = Delivery::new(
        delivery_id.clone(),
        DeliveryData {
            delivery_no: crate::fulfillment::document_number::next_delivery_no(db).await?,
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
    let lines = build_stock_delivery_lines(&delivery_id, reservations, 1)?;
    db.fulfillment()
        .create_delivery_with_lines(&delivery, &lines, session)
        .await?;
    crate::fulfillment::task::ensure_fulfillment_task(
        db,
        crate::fulfillment::task::FulfillmentTaskObject::Delivery(&delivery),
        session,
    )
    .await
}

/// 向现有仓发草稿追加尚未出现的库存预占行。
async fn append_stock_delivery_lines(
    db: &mongodb::Database,
    delivery: &Delivery,
    reservations: &[&StockReservation],
    session: &mut ClientSession,
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
    for line in build_stock_delivery_lines(&delivery_id, &pending, next_line_no)? {
        db.delivery_lines().create(&line, session).await?;
    }
    Ok(())
}

/// 将库存预占投影为仓发草稿行。
fn build_stock_delivery_lines(
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

/// 写入整批选源创建命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `audit_id` - 稳定收据 ID
/// * `request_fingerprint` - 当前命令载荷指纹
/// * `sales_order_id` - 来源销售单，作为收据资源身份
/// * `receipt` - 采购单与库存预留的完整命令结果
/// * `actor` - 审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 写入成功返回 `Ok(())`。
///
/// # 错误
/// 收据序列化或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 收据与全部采购单及库存预留必须同事务提交。
async fn write_sourcing_receipt(
    db: &mongodb::Database,
    audit_id: &str,
    request_fingerprint: &str,
    sales_order_id: &str,
    receipt: &SourcingReceipt,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let audit = actor.clone().resource_log_with_id(
        audit_id.to_string(),
        CREATE_SOURCING_ACTION,
        "purchase_order",
        sales_order_id.to_string(),
        Some(PurchaseCommandReceipt::new(request_fingerprint.to_string(), receipt.clone()).encode_message()?),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(())
}

/// 查询并校验选源创建幂等收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `audit_id` - 稳定收据 ID
/// * `expected_fingerprint` - 当前命令载荷指纹
/// * `actor` - 当前操作人
/// * `sales_order_id` - 来源销售单
/// * `work_item_id` - 发起该命令的供给分配工作项
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 收据不存在返回 `None`；存在且一致返回原创建结果并标记回放。
///
/// # 错误
/// 同键异载荷、收据身份不一致或收据损坏时返回错误。
///
/// # 关键业务约束
/// 事务前、事务内和事务失败后均复用同一校验逻辑。
async fn replay_sourcing(
    db: &mongodb::Database,
    audit_id: &str,
    expected_fingerprint: &str,
    actor: &AuditActor,
    sales_order_id: &str,
    work_item_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<CreatePurchaseOrdersFromSourcingResult>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, executor).await? else {
        return Ok(None);
    };
    let receipt = match PurchaseCommandReceipt::<SourcingReceipt>::decode(
        &audit,
        actor.id(),
        CREATE_SOURCING_ACTION,
        Some(sales_order_id),
        expected_fingerprint,
    ) {
        Ok(receipt) => receipt,
        Err(PurchaseCommandReceiptError::IdentityMismatch | PurchaseCommandReceiptError::PayloadConflict) => {
            return Err(Error::ConflictError("幂等键已用于不同采购命令".to_string()));
        }
        Err(PurchaseCommandReceiptError::Corrupted(message)) => {
            return Err(Error::Internal(message));
        }
    }
    .into_payload();
    let work_item_status = sourcing_receipt_work_item_status(
        db,
        receipt.work_item_status,
        work_item_id,
        sales_order_id,
        executor,
    )
    .await?;
    Ok(Some(CreatePurchaseOrdersFromSourcingResult {
        orders: receipt
            .orders
            .into_iter()
            .map(|order| CreatePurchaseOrderResult {
                purchase_order_id: order.purchase_order_id.clone(),
                purchase_no: order.purchase_no,
                lock_version: order.lock_version,
                replayed: true,
                reference: order.purchase_order_id,
            })
            .collect(),
        stock_reservations: receipt.stock_reservations,
        work_item_status,
        replayed: true,
        reference: sales_order_id.to_string(),
    }))
}

/// 解析新旧选源收据中的工作项状态。
///
/// 新收据冻结命令提交时的状态。旧收据没有该字段，优先按收据指向的同一工作项
/// 读取当前事实；历史任务不存在时，已提交成功的旧命令按终态返回。
async fn sourcing_receipt_work_item_status(
    db: &mongodb::Database,
    frozen_status: Option<WorkItemStatus>,
    work_item_id: &str,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<String> {
    if let Some(status) = frozen_status {
        return sourcing_work_item_status(status, false);
    }
    let Some(item) = db.work_items().find_by_id(work_item_id, executor).await? else {
        return Ok(WorkItemStatus::Completed.as_str().to_string());
    };
    if item.work_item_type != WorkItemType::ProcurementOrderCreation
        || item.business_object_type != "sales_order"
        || item.business_object_id != sales_order_id
    {
        return Err(Error::Internal(
            "旧版选源幂等收据对应的工作项身份非法".to_string(),
        ));
    }
    sourcing_work_item_status(item.status, true)
}

/// 将供给分配任务状态投影为客户端结果合同。
fn sourcing_work_item_status(status: WorkItemStatus, legacy: bool) -> Result<String> {
    match status {
        WorkItemStatus::Open => Ok(WorkItemStatus::Open.as_str().to_string()),
        WorkItemStatus::Completed => Ok(WorkItemStatus::Completed.as_str().to_string()),
        WorkItemStatus::Closed if legacy => Ok(WorkItemStatus::Completed.as_str().to_string()),
        WorkItemStatus::Closed => Err(Error::Internal("选源幂等收据中的任务状态非法".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::{sourcing_work_item_status, SourcingReceipt};
    use entities::work_item::WorkItemStatus;

    /// 幂等回放必须保留同步后的任务状态，不能把部分分配误报为任务完成。
    #[test]
    fn sourcing_receipt_freezes_work_item_status() {
        let receipt = SourcingReceipt {
            orders: Vec::new(),
            stock_reservations: Vec::new(),
            work_item_status: Some(WorkItemStatus::Open),
        };

        let encoded = serde_json::to_string(&receipt).expect("选源回执必须可序列化");
        let replayed: SourcingReceipt = serde_json::from_str(&encoded).expect("选源回执必须可回放");

        assert_eq!(replayed.work_item_status, Some(WorkItemStatus::Open));
    }

    /// 旧版收据缺少任务状态时必须可解码，并由回放路径恢复其生命周期结果。
    #[test]
    fn legacy_sourcing_receipt_without_work_item_status_decodes() {
        let replayed: SourcingReceipt =
            serde_json::from_str(r#"{"orders":[],"stock_reservations":[]}"#).expect("旧版选源回执必须可回放");

        assert_eq!(replayed.work_item_status, None);
        assert_eq!(
            sourcing_work_item_status(WorkItemStatus::Closed, true).unwrap(),
            "COMPLETED"
        );
        assert!(sourcing_work_item_status(WorkItemStatus::Closed, false).is_err());
    }

    /// 验证选源创建的操作人授权提交栅栏。
    #[test]
    fn create_from_sourcing_binds_actor_authorization_to_commit() {
        let production = include_str!("sourcing_create.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");
        assert!(production.contains("authorize_actor_permission(actor, CREATE_PERMISSION)"));
        assert!(production.contains("ensure_purchase_order_actor_account"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
        assert!(production.contains("advance_procurement_guard"));
        assert!(production.contains("reserve_quantity"));
        assert!(production.contains("StockReservationSourceType::ExistingStock"));
        assert!(production.contains("create_stock_delivery_drafts"));
        assert!(production.contains("plan.target_warehouse_id.as_ref()"));
        assert!(production.contains("persist_basis_draft"));
        assert!(production.contains("sync_procurement_tasks_for_sales_order"));
    }
}
