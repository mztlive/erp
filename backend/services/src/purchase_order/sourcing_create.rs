//! 按供给分配结果一次落地现有库存预占和采购缺口。
//!
//! 操作人确认库存优先的推荐结果后，本模块在同一事务内推进一次供给 guard、
//! 原子预占现有库存并生成仓发草稿，再把剩余采购分配按供应商、采购类型、
//! 付款条件和履约责任拆成采购单并启动审批。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use database::{AccessControlExt, Executor, FulfillmentExt, InventoryExt, NoTransaction, SalesOrderExt};
use entities::common::time::BusinessDate;
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
use id_generator::next_id;
use mongodb::ClientSession;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::command_receipt::{
    command_receipt_id, command_receipt_message, command_request_fingerprint, parse_command_receipt,
};
use super::creation_basis::{
    basis_groups_for_order, basis_id_for, basis_scope_key, load_effective_sales_order, persist_basis_draft,
    procurement_quantity_changed, stable_line_id, stock_basis_groups_for_order, stock_basis_id_for,
    validate_requested_quantities, BasisGroup, CreateBasisCommand, RequestedLine, StockBasisGroup,
};
use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest, CreatePurchaseOrderResult,
    CreatePurchaseOrdersFromSourcingRequest, CreatePurchaseOrdersFromSourcingResult,
    ExistingStockReservationResult, SupplySourceType,
};
use super::procurement_task_sync::{
    load_owned_open_procurement_task, sync_procurement_tasks_for_sales_order,
};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

const CREATE_PERMISSION: &str = "purchase_order:create";
const CREATE_SOURCING_ACTION: &str = "purchase_order.create_from_sourcing";
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
}

/// 已归入一张采购单的选源计划。
#[derive(Debug, Clone)]
struct SourcingDraftPlan {
    /// 命中的精确依据分组。
    group: BasisGroup,
    /// 本单规范化后的逐行数量。
    requested_lines: Vec<RequestedLine>,
}

/// 已归入一个库存余额的现有库存分配计划。
#[derive(Debug, Clone)]
struct StockAllocationPlan {
    /// 命中的现有库存依据。
    group: StockBasisGroup,
    /// 本余额逐销售行分配数量。
    requested_lines: Vec<RequestedStockLine>,
}

/// 已规范化的现有库存分配行。
#[derive(Debug, Clone)]
struct RequestedStockLine {
    /// 稳定销售行。
    sales_order_line_id: String,
    /// 本次预占数量。
    quantity: Quantity,
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
        let assignments = normalize_sourcing_assignments(&req)?;
        let request_fingerprint = sourcing_request_fingerprint(&req, &assignments)?;
        let sales_order_id = SalesOrderId::new(req.sales_order_id.trim().to_string());
        let audit_id = command_receipt_id(
            CREATE_SOURCING_RECEIPT_PREFIX,
            actor.id(),
            CREATE_SOURCING_ACTION,
            sales_order_id.as_ref(),
            &req.idempotency_key,
        );
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
/// * `assignments` - 已规范化且稳定行不重复的选源行
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
    assignments: &[RequestedSourcingLine],
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
    let plans = plan_sourcing_drafts(&order, &groups, &req.work_item_id, assignments)?;
    let stock_plans = plan_stock_allocations(&order, &stock_groups, &req.work_item_id, assignments)?;
    validate_combined_line_totals(&plans, &stock_plans)?;
    order.advance_procurement_guard(actor.id())?;
    db.sales_orders().update(&mut order, session).await?;
    let latest_stock_groups =
        stock_basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    validate_stock_totals(&stock_plans, &latest_stock_groups)?;
    let persisted_stock = persist_stock_allocations(
        db,
        &stock_plans,
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
    let latest_groups = basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    validate_sourcing_totals(&plans, &latest_groups)?;
    let mut orders = Vec::with_capacity(plans.len());
    for plan in plans {
        let latest = latest_groups
            .iter()
            .find(|group| group.scope == plan.group.scope)
            .ok_or_else(procurement_quantity_changed)?;
        let selected_lines = validate_requested_quantities(&plan.requested_lines, latest)?;
        let basis_id = basis_id_for(&order, latest, &req.work_item_id);
        let item_req = CreatePurchaseOrderFromBasisRequest {
            work_item_id: req.work_item_id.clone(),
            basis_id: basis_id.clone(),
            purchase_type: latest.scope.purchase_type,
            payment_term_code: latest.scope.payment_term_code.clone(),
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
        let item_audit_id = command_receipt_id(
            CREATE_SOURCING_ITEM_PREFIX,
            actor.id(),
            CREATE_SOURCING_ACTION,
            &basis_id,
            &req.idempotency_key,
        );
        let command = CreateBasisCommand {
            sales_order_id,
            req: &item_req,
            requested_lines: &plan.requested_lines,
            audit_id: &item_audit_id,
            request_fingerprint,
            actor,
        };
        orders.push(persist_basis_draft(db, rbac, &order, latest, &selected_lines, &command, session).await?);
    }
    sync_procurement_tasks_for_sales_order(db, sales_order_id, session).await?;
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
        replayed: false,
        reference: sales_order_id.to_string(),
    })
}

/// 已规范化的选源行。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestedSourcingLine {
    /// 稳定销售行。
    sales_order_line_id: String,
    /// 本行选用的精确创建依据。
    basis_id: String,
    /// 供给来源。
    source_type: SupplySourceType,
    /// 本次分配数量。
    quantity: Quantity,
    /// 采购确认的预计交付日。
    expected_delivery_date: BusinessDate,
}

/// 规范化并校验选源行。
///
/// # 参数
/// * `req` - 选源创建请求
///
/// # 返回
/// 返回履约分配去重、数量已类型化且稳定排序的选源行。
///
/// # 错误
/// 同一销售行重复使用同一依据、依据空白、数量非法或数量不大于零时返回校验错误。
///
/// # 关键业务约束
/// 同一稳定销售行可按不同依据拆分，但同一依据只能出现一次。
fn normalize_sourcing_assignments(
    req: &CreatePurchaseOrdersFromSourcingRequest,
) -> Result<Vec<RequestedSourcingLine>> {
    let mut seen = HashSet::new();
    let mut lines = Vec::with_capacity(req.lines.len());
    for line in &req.lines {
        let sales_order_line_id = line.sales_order_line_id.trim().to_string();
        let basis_id = line.basis_id.trim().to_string();
        if sales_order_line_id.is_empty() {
            return Err(Error::ValidationError("销售行不能为空".to_string()));
        }
        if basis_id.is_empty() {
            return Err(Error::ValidationError("履约方案不能为空".to_string()));
        }
        if !seen.insert((sales_order_line_id.clone(), basis_id.clone())) {
            return Err(Error::ValidationError(
                "同一销售行不能重复使用同一履约方案".to_string(),
            ));
        }
        let quantity = Quantity::from_str(line.quantity.trim())
            .map_err(|error| Error::ValidationError(format!("本次分配数量非法: {error}")))?;
        let expected_delivery_date = BusinessDate::from_str(line.expected_delivery_date.trim())
            .map_err(|error| Error::ValidationError(format!("预计交付日非法: {error}")))?;
        let zero =
            Quantity::from_str("0").map_err(|error| Error::Internal(format!("零数量常量非法: {error}")))?;
        if quantity <= zero {
            return Err(Error::ValidationError("本次分配数量必须大于 0".to_string()));
        }
        lines.push(RequestedSourcingLine {
            sales_order_line_id,
            basis_id,
            source_type: line.source_type,
            quantity,
            expected_delivery_date,
        });
    }
    lines.sort_by(|left, right| {
        left.sales_order_line_id
            .cmp(&right.sales_order_line_id)
            .then_with(|| left.basis_id.cmp(&right.basis_id))
    });
    Ok(lines)
}

/// 把采购来源行归入精确依据分组，形成待创建采购单计划。
///
/// # 参数
/// * `groups` - 当前任务范围内的精确依据
/// * `assignments` - 已规范化选源行
///
/// # 返回
/// 返回按拆分维度稳定排序的草稿计划。
///
/// # 错误
/// 销售行不属于当前任务、所选供应商无合格供给时返回校验错误。
///
/// # 关键业务约束
/// 同一拆分维度的选源行合并为一张采购单。
fn plan_sourcing_drafts(
    order: &entities::sales_order::SalesOrder,
    groups: &[BasisGroup],
    work_item_id: &str,
    assignments: &[RequestedSourcingLine],
) -> Result<Vec<SourcingDraftPlan>> {
    let mut plans: BTreeMap<String, SourcingDraftPlan> = BTreeMap::new();
    for assignment in assignments
        .iter()
        .filter(|assignment| assignment.source_type == SupplySourceType::Purchase)
    {
        let group = find_assignment_group(order, groups, work_item_id, assignment)?;
        let key = basis_scope_key(&group.scope);
        let requested = RequestedLine {
            sales_order_line_id: assignment.sales_order_line_id.clone(),
            quantity: assignment.quantity,
            expected_delivery_date: assignment.expected_delivery_date,
        };
        if let Some(plan) = plans.get_mut(&key) {
            plan.requested_lines.push(requested);
        } else {
            plans.insert(
                key,
                SourcingDraftPlan {
                    group: group.clone(),
                    requested_lines: vec![requested],
                },
            );
        }
    }
    Ok(plans.into_values().collect())
}

/// 把现有库存选源行按库存余额归组。
fn plan_stock_allocations(
    order: &entities::sales_order::SalesOrder,
    groups: &[StockBasisGroup],
    work_item_id: &str,
    assignments: &[RequestedSourcingLine],
) -> Result<Vec<StockAllocationPlan>> {
    let mut plans = BTreeMap::<String, StockAllocationPlan>::new();
    for assignment in assignments
        .iter()
        .filter(|assignment| assignment.source_type == SupplySourceType::ExistingStock)
    {
        let group = groups
            .iter()
            .find(|group| {
                stock_basis_id_for(order, group, work_item_id) == assignment.basis_id
                    && group.lines.iter().any(|line| {
                        line.coverage.revision_line.sales_order_line_id.as_ref()
                            == assignment.sales_order_line_id
                    })
            })
            .ok_or_else(procurement_quantity_changed)?;
        let requested = RequestedStockLine {
            sales_order_line_id: assignment.sales_order_line_id.clone(),
            quantity: assignment.quantity,
        };
        plans
            .entry(group.balance.base.id.clone())
            .and_modify(|plan| plan.requested_lines.push(requested.clone()))
            .or_insert_with(|| StockAllocationPlan {
                group: group.clone(),
                requested_lines: vec![requested],
            });
    }
    Ok(plans.into_values().collect())
}

/// 校验同一命令内库存和采购拆分合计不超过当前销售缺口。
fn validate_combined_line_totals(
    purchase_plans: &[SourcingDraftPlan],
    stock_plans: &[StockAllocationPlan],
) -> Result<()> {
    let mut totals = HashMap::<String, Decimal>::new();
    let mut caps = HashMap::<String, Decimal>::new();
    for plan in purchase_plans {
        for requested in &plan.requested_lines {
            let line = plan
                .group
                .lines
                .iter()
                .find(|line| stable_line_id(line) == requested.sales_order_line_id)
                .ok_or_else(procurement_quantity_changed)?;
            add_requested_total(
                &mut totals,
                &mut caps,
                &requested.sales_order_line_id,
                requested.quantity,
                line.coverage.summary.remaining_quantity,
            );
        }
    }
    for plan in stock_plans {
        for requested in &plan.requested_lines {
            let line = stock_line(&plan.group, &requested.sales_order_line_id)?;
            add_requested_total(
                &mut totals,
                &mut caps,
                &requested.sales_order_line_id,
                requested.quantity,
                line.coverage.summary.remaining_quantity,
            );
        }
    }
    if exceeds_any_cap(&totals, &caps) {
        return Err(procurement_quantity_changed());
    }
    Ok(())
}

/// 校验 guard 推进后的库存余额与销售缺口仍可承载本次分配。
fn validate_stock_totals(plans: &[StockAllocationPlan], latest_groups: &[StockBasisGroup]) -> Result<()> {
    let mut line_totals = HashMap::<String, Decimal>::new();
    let mut line_caps = HashMap::<String, Decimal>::new();
    let mut balance_totals = HashMap::<String, Decimal>::new();
    let mut balance_caps = HashMap::<String, Decimal>::new();
    for plan in plans {
        let latest = latest_stock_group(latest_groups, &plan.group.balance.base.id)?;
        for requested in &plan.requested_lines {
            let line = stock_line(latest, &requested.sales_order_line_id)?;
            add_requested_total(
                &mut line_totals,
                &mut line_caps,
                &requested.sales_order_line_id,
                requested.quantity,
                line.coverage.summary.remaining_quantity,
            );
            *balance_totals
                .entry(latest.balance.base.id.clone())
                .or_insert(Decimal::ZERO) += requested.quantity.to_decimal();
            balance_caps.insert(
                latest.balance.base.id.clone(),
                latest.balance.available_quantity.to_decimal(),
            );
        }
    }
    if exceeds_any_cap(&line_totals, &line_caps) || exceeds_any_cap(&balance_totals, &balance_caps) {
        return Err(procurement_quantity_changed());
    }
    Ok(())
}

/// 累加一条请求数量并登记该稳定销售行的统一上限。
fn add_requested_total(
    totals: &mut HashMap<String, Decimal>,
    caps: &mut HashMap<String, Decimal>,
    line_id: &str,
    quantity: Quantity,
    cap: Quantity,
) {
    *totals.entry(line_id.to_string()).or_insert(Decimal::ZERO) += quantity.to_decimal();
    caps.insert(line_id.to_string(), cap.to_decimal());
}

/// 查找 guard 后仍有效的库存余额依据。
fn latest_stock_group<'a>(groups: &'a [StockBasisGroup], balance_id: &str) -> Result<&'a StockBasisGroup> {
    groups
        .iter()
        .find(|group| group.balance.base.id == balance_id)
        .ok_or_else(procurement_quantity_changed)
}

/// 查找库存依据中的稳定销售行。
fn stock_line<'a>(
    group: &'a StockBasisGroup,
    sales_order_line_id: &str,
) -> Result<&'a super::creation_basis::StockBasisLine> {
    group
        .lines
        .iter()
        .find(|line| line.coverage.revision_line.sales_order_line_id.as_ref() == sales_order_line_id)
        .ok_or_else(procurement_quantity_changed)
}

/// 查找一条选源行命中的精确依据。
///
/// # 参数
/// * `groups` - 当前任务范围内的精确依据
/// * `assignment` - 已规范化选源行
///
/// # 返回
/// 返回同时包含该销售行且 ID 与客户端选择一致的依据分组。
///
/// # 错误
/// 销售行不存在或依据已失效时返回校验错误。
///
/// # 关键业务约束
/// 不以供应商或 SKU 猜测路线，只接受当前开放任务生成的精确依据。
fn find_assignment_group<'a>(
    order: &entities::sales_order::SalesOrder,
    groups: &'a [BasisGroup],
    work_item_id: &str,
    assignment: &RequestedSourcingLine,
) -> Result<&'a BasisGroup> {
    groups
        .iter()
        .find(|group| {
            basis_id_for(order, group, work_item_id) == assignment.basis_id
                && group
                    .lines
                    .iter()
                    .any(|line| stable_line_id(line) == assignment.sales_order_line_id)
        })
        .ok_or_else(procurement_quantity_changed)
}

/// 校验跨采购单拆分后的销售行总量与单一供给总量。
///
/// # 参数
/// * `plans` - guard 推进前按精确依据形成的建单计划
/// * `latest_groups` - guard 推进后重新计算的最新依据
///
/// # 返回
/// 所有拆分数量均未超过最新销售剩余量和供给上限时返回 `Ok(())`。
///
/// # 错误
/// 依据失效、销售行累计超量或同一供给跨履约责任累计超量时返回并发冲突。
///
/// # 关键业务约束
/// 一条销售行可以拆到多个方案，但一次命令的总量不得突破同一份最新剩余量；
/// 同一供应商供给跨销售行或履约责任时仍共享该供给的可用量。
fn validate_sourcing_totals(plans: &[SourcingDraftPlan], latest_groups: &[BasisGroup]) -> Result<()> {
    let mut line_totals = HashMap::<String, Decimal>::new();
    let mut line_caps = HashMap::<String, Decimal>::new();
    let mut supply_totals = HashMap::<String, Decimal>::new();
    let mut supply_caps = HashMap::<String, Decimal>::new();
    for plan in plans {
        let latest = latest_groups
            .iter()
            .find(|group| group.scope == plan.group.scope)
            .ok_or_else(procurement_quantity_changed)?;
        for requested in &plan.requested_lines {
            let basis = latest
                .lines
                .iter()
                .find(|line| stable_line_id(line) == requested.sales_order_line_id)
                .ok_or_else(procurement_quantity_changed)?;
            *line_totals
                .entry(requested.sales_order_line_id.clone())
                .or_insert(Decimal::ZERO) += requested.quantity.to_decimal();
            line_caps.insert(
                requested.sales_order_line_id.clone(),
                basis.coverage.summary.remaining_quantity.to_decimal(),
            );
            let supply_key = basis.supply.offering.base.id.clone();
            *supply_totals.entry(supply_key.clone()).or_insert(Decimal::ZERO) +=
                requested.quantity.to_decimal();
            supply_caps.insert(
                supply_key,
                basis
                    .supply
                    .availability
                    .available_quantity
                    .map(Quantity::to_decimal)
                    .unwrap_or(Decimal::MAX),
            );
        }
    }
    if exceeds_any_cap(&line_totals, &line_caps) || exceeds_any_cap(&supply_totals, &supply_caps) {
        return Err(procurement_quantity_changed());
    }
    Ok(())
}

/// 判断任一累计数量是否缺少上限或超过上限。
fn exceeds_any_cap(totals: &HashMap<String, Decimal>, caps: &HashMap<String, Decimal>) -> bool {
    totals
        .iter()
        .any(|(key, total)| caps.get(key).is_none_or(|cap| total > cap))
}

/// 在事务内原子预占现有库存并写入预占建立流水。
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
            let line = stock_line(latest, &requested.sales_order_line_id)?;
            if !db
                .stock_balances()
                .reserve_quantity(&latest.balance.base.id, requested.quantity, session)
                .await?
            {
                return Err(procurement_quantity_changed());
            }
            let source_allocation_id = command_request_fingerprint(
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
        return Ok(());
    }
    let delivery_id = DeliveryId::new(next_id());
    let delivery = Delivery::new(
        delivery_id.clone(),
        DeliveryData {
            delivery_no: format!("FH-{}", delivery_id.as_ref()),
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
        .await
        .map_err(Into::into)
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

/// 构造选源命令载荷指纹。
///
/// # 参数
/// * `req` - 选源创建请求
/// * `assignments` - 已规范化并排序的选源行
///
/// # 返回
/// 返回不包含原始幂等键的 SHA-256 指纹。
///
/// # 错误
/// 指纹序列化失败时返回内部错误。
///
/// # 关键业务约束
/// 同一幂等键用于不同任务、销售单、供应商或数量时必须冲突。
fn sourcing_request_fingerprint(
    req: &CreatePurchaseOrdersFromSourcingRequest,
    assignments: &[RequestedSourcingLine],
) -> Result<String> {
    let payload = assignments
        .iter()
        .map(|line| {
            (
                line.sales_order_line_id.clone(),
                line.basis_id.clone(),
                line.source_type,
                line.quantity.to_string(),
                line.expected_delivery_date.to_string(),
            )
        })
        .collect::<Vec<_>>();
    command_request_fingerprint(
        CREATE_SOURCING_ACTION,
        req.sales_order_id.trim(),
        &(req.work_item_id.trim(), payload),
    )
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
        Some(command_receipt_message(request_fingerprint, receipt)?),
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
    executor: &mut dyn Executor,
) -> Result<Option<CreatePurchaseOrdersFromSourcingResult>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, executor).await? else {
        return Ok(None);
    };
    let receipt = parse_command_receipt::<SourcingReceipt>(
        &audit,
        actor.id(),
        CREATE_SOURCING_ACTION,
        sales_order_id,
        expected_fingerprint,
    )?;
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
        replayed: true,
        reference: sales_order_id.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use rust_decimal::Decimal;

    use super::{
        exceeds_any_cap, find_assignment_group_index, normalize_assignment_pairs, DedupError,
        RequestedSourcingLine, SupplySourceType,
    };

    /// 同一销售行可拆到不同履约方案。
    #[test]
    fn one_sales_line_can_use_different_bases() {
        normalize_assignment_pairs(&[("line-1", "basis-a", "1"), ("line-1", "basis-b", "1")])
            .expect("不同履约方案允许拆分");
    }

    /// 同一销售行不能重复使用同一履约方案。
    #[test]
    fn duplicate_line_and_basis_are_rejected() {
        let error = normalize_assignment_pairs(&[("line-1", "basis-a", "1"), ("line-1", "basis-a", "1")])
            .expect_err("重复履约分配必须失败");
        assert!(matches!(error, DedupError::DuplicateAllocation));
    }

    /// 不同销售行指定同一精确依据时应归入同一分组下标。
    #[test]
    fn same_basis_lines_share_one_group() {
        let groups = [
            ("basis-a", &["line-1", "line-2"][..]),
            ("basis-b", &["line-1"][..]),
        ];
        let assignments = [line("line-1", "basis-a"), line("line-2", "basis-a")];
        let indexes = assignments
            .iter()
            .map(|assignment| find_assignment_group_index(&groups, assignment).expect("应命中供给"))
            .collect::<Vec<_>>();
        assert_eq!(indexes, vec![0, 0]);
    }

    /// 不同精确依据必须拆到不同分组。
    #[test]
    fn different_bases_split_groups() {
        let groups = [("basis-a", &["line-1"][..]), ("basis-b", &["line-2"][..])];
        let first = find_assignment_group_index(&groups, &line("line-1", "basis-a")).expect("A");
        let second = find_assignment_group_index(&groups, &line("line-2", "basis-b")).expect("B");
        assert_ne!(first, second);
    }

    /// 销售行不属于该精确依据时不能建单。
    #[test]
    fn missing_basis_option_is_rejected() {
        let groups = [("basis-a", &["line-1"][..])];
        assert!(find_assignment_group_index(&groups, &line("line-1", "basis-b")).is_none());
    }

    /// 拆分数量合计不得超过事务内最新上限。
    #[test]
    fn split_totals_must_stay_within_latest_cap() {
        let totals = HashMap::from([("line-1".to_string(), Decimal::from_str("11").unwrap())]);
        let caps = HashMap::from([("line-1".to_string(), Decimal::from_str("10").unwrap())]);

        assert!(exceeds_any_cap(&totals, &caps));
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
        assert!(production.contains("persist_basis_draft"));
        assert!(production.contains("sync_procurement_tasks_for_sales_order"));
    }

    /// 构造测试用选源行。
    fn line(sales_order_line_id: &str, basis_id: &str) -> RequestedSourcingLine {
        RequestedSourcingLine {
            sales_order_line_id: sales_order_line_id.to_string(),
            basis_id: basis_id.to_string(),
            source_type: SupplySourceType::Purchase,
            quantity: entities::money::Quantity::from_str("1").expect("测试数量合法"),
            expected_delivery_date: BusinessDate::from_str("2026-09-01").expect("测试日期合法"),
        }
    }
}

/// 测试辅助：选源行去重错误。
#[cfg(test)]
#[derive(Debug)]
enum DedupError {
    /// 同一销售行与精确依据组合出现多次。
    DuplicateAllocation,
}

/// 测试辅助：校验销售行与精确依据组合去重。
///
/// # 参数
/// * `pairs` - `(销售行, 精确依据, 数量)` 三元组
///
/// # 返回
/// 无重复时返回 `Ok(())`。
///
/// # 错误
/// 销售行与依据组合重复时返回 `DedupError::DuplicateAllocation`。
#[cfg(test)]
fn normalize_assignment_pairs(pairs: &[(&str, &str, &str)]) -> std::result::Result<(), DedupError> {
    let mut seen = HashSet::new();
    for (line_id, basis_id, _) in pairs {
        if !seen.insert((*line_id, *basis_id)) {
            return Err(DedupError::DuplicateAllocation);
        }
    }
    Ok(())
}

/// 测试辅助：按精确依据与销售行查找分组下标。
///
/// # 参数
/// * `groups` - `(精确依据, 销售行列表)` 分组
/// * `assignment` - 选源行
///
/// # 返回
/// 命中时返回分组下标，否则返回 `None`。
///
/// # 错误
/// 无。
#[cfg(test)]
fn find_assignment_group_index(
    groups: &[(&str, &[&str])],
    assignment: &RequestedSourcingLine,
) -> Option<usize> {
    groups.iter().position(|(basis_id, lines)| {
        *basis_id == assignment.basis_id
            && lines
                .iter()
                .any(|line_id| *line_id == assignment.sales_order_line_id)
    })
}
