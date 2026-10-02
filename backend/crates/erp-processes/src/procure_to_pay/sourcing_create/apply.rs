//! 按原事务阶段顺序落地供给计划，所有读写使用调用方 Executor。

use erp_procurement::entity::purchase_order::{BasisGroup, SourcingDraftPlan};
use erp_sales::entity::sales_order::SalesOrder;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;

use super::{
    CREATE_SOURCING_ACTION, CREATE_SOURCING_ITEM_PREFIX, CreateBasisCommand, CreateFromSourcingApplyInput,
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest, CreatePurchaseOrderResult,
    CreatePurchaseOrdersFromSourcingResult, Error, Executor, ExistingStockReservationResult,
    LegacyReceiptIdScheme, PurchaseCommandReceipt, Result, SalesOrderExt, SourcingOrderReceipt, SourcingPlan,
    SourcingReceipt, VerifiedBasisInput, WorkItemExt, basis_groups_and_facts, basis_id_for,
    create_stock_delivery_drafts, load_effective_sales_order, load_owned_open_procurement_task,
    map_sourcing_plan_error, persist_basis_draft, persist_stock_allocations, procurement_quantity_changed,
    replay_sourcing, sales_order_basis_fact, sourcing_groups_for_order, sourcing_work_item_status,
    stock_basis_groups_for_order, sync_procurement_tasks_for_sales_order, validate_requested_quantities,
    write_sourcing_receipt,
};

/// 按任务、计划、guard、库存、采购和收据的原顺序执行事务内命令。
///
/// # 参数
/// * `db` - 数据库。
/// * `input` - 已规范化请求、授权、审计和幂等上下文。
/// * `executor` - 本次事务的原执行器。
/// # 返回
/// 返回首次执行或事务内幂等回放结果。
/// # 错误
/// 首个任务、计划、版本、库存、采购、审批或持久化错误原样传播。
pub(super) async fn create_from_sourcing_apply(
    db: &Database,
    input: CreateFromSourcingApplyInput<'_>,
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrdersFromSourcingResult> {
    if let Some(result) = replay_sourcing(
        db,
        input.audit_id,
        input.request_fingerprint,
        input.actor,
        input.sales_order_id.as_ref(),
        &input.req.work_item_id,
        executor,
    )
    .await?
    {
        return Ok(result);
    }
    let (task, order, plan) = prepare_plan(db, &input, executor).await?;
    let stock_reservations = reserve_stock(db, &input, &task, &order, &plan, executor).await?;
    let orders = create_orders(db, &input, &task, &order, &plan, executor).await?;
    finish_sourcing(db, &input, orders, stock_reservations, executor).await
}

/// guard 前形成计划，再按原顺序推进一次销售单供给 guard。
async fn prepare_plan(
    db: &Database,
    input: &CreateFromSourcingApplyInput<'_>,
    executor: &mut dyn Executor,
) -> Result<(WorkItem, SalesOrder, SourcingPlan)> {
    let task = load_owned_open_procurement_task(
        db,
        &input.req.work_item_id,
        input.sales_order_id,
        input.actor.id(),
        executor,
    )
    .await?;
    let mut order = load_effective_sales_order(db, input.sales_order_id, executor).await?;
    let (groups, stock_groups) =
        sourcing_groups_for_order(db, &order, task.responsibility_scope_ids(), executor).await?;
    let plan = SourcingPlan::plan(
        &sales_order_basis_fact(&order),
        &groups,
        &stock_groups,
        &input.req.work_item_id,
        input.assignments,
    )
    .map_err(map_sourcing_plan_error)?;
    order.advance_procurement_guard(input.actor.id())?;
    db.sales_orders().update(&mut order, executor).await?;
    Ok((task, order, plan))
}

/// guard 后重新取库存依据，预占并生成仓发草稿，再返回实际登记的结果。
async fn reserve_stock(
    db: &Database,
    input: &CreateFromSourcingApplyInput<'_>,
    task: &WorkItem,
    order: &SalesOrder,
    plan: &SourcingPlan,
    executor: &mut dyn Executor,
) -> Result<Vec<ExistingStockReservationResult>> {
    let latest_stock_groups =
        stock_basis_groups_for_order(db, order, task.responsibility_scope_ids(), executor).await?;
    plan.validate_against_latest_stock(&latest_stock_groups).map_err(map_sourcing_plan_error)?;
    let persisted = persist_stock_allocations(
        db,
        plan.stock_plans(),
        &latest_stock_groups,
        input.sales_order_id,
        input.audit_id,
        input.request_fingerprint,
        executor,
    )
    .await?;
    create_stock_delivery_drafts(db, input.sales_order_id, &persisted, executor).await?;
    Ok(persisted.into_iter().map(|allocation| allocation.result).collect())
}

/// 库存写入后重新取采购依据，逐单保留数量复验及完整持久化顺序。
async fn create_orders(
    db: &Database,
    input: &CreateFromSourcingApplyInput<'_>,
    task: &WorkItem,
    order: &SalesOrder,
    plan: &SourcingPlan,
    executor: &mut dyn Executor,
) -> Result<Vec<CreatePurchaseOrderResult>> {
    let (latest_groups, latest_facts) =
        basis_groups_and_facts(db, order, task.responsibility_scope_ids(), executor).await?;
    plan.validate_against_latest_sourcing(&latest_groups).map_err(map_sourcing_plan_error)?;
    let mut orders = Vec::with_capacity(plan.purchase_plans().len());
    for draft in plan.purchase_plans() {
        let latest = latest_groups
            .iter()
            .find(|group| group.scope == draft.group.scope)
            .ok_or_else(procurement_quantity_changed)?;
        let selected_lines = validate_requested_quantities(&draft.requested_lines, latest)?;
        let verified = VerifiedBasisInput {
            sales_order: order,
            group: latest,
            selected_lines: &selected_lines,
            facts: &latest_facts,
        };
        orders.push(persist_order(db, input, &verified, draft, executor).await?);
    }
    Ok(orders)
}

/// 构造单张采购命令的收据，再沿原 Executor 登记草稿、冻结和审批。
async fn persist_order(
    db: &Database,
    input: &CreateFromSourcingApplyInput<'_>,
    verified: &VerifiedBasisInput<'_>,
    plan: &SourcingDraftPlan,
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrderResult> {
    let basis_id = basis_id_for(
        &sales_order_basis_fact(verified.sales_order),
        verified.group,
        &input.req.work_item_id,
        plan.target_warehouse_id.as_ref(),
    );
    let item_req = item_request(input, verified.group, plan, &basis_id);
    let identity = PurchaseCommandReceipt::<SourcingReceipt>::identity(
        CREATE_SOURCING_ITEM_PREFIX,
        input.actor.id(),
        CREATE_SOURCING_ACTION,
        Some(basis_id.as_str()),
        &input.req.idempotency_key,
        LegacyReceiptIdScheme::None,
    )?;
    let audit_id = identity.receipt_id().to_string();
    let command = CreateBasisCommand {
        sales_order_id: input.sales_order_id,
        req: &item_req,
        requested_lines: &plan.requested_lines,
        audit_id: &audit_id,
        request_fingerprint: input.request_fingerprint,
        actor: input.actor,
    };
    persist_basis_draft(db, input.rbac, input.object_read, verified, &command, executor).await
}

/// 按原字段和行顺序组装单张采购请求，不改变规范化或首错处理。
fn item_request(
    input: &CreateFromSourcingApplyInput<'_>,
    latest: &BasisGroup,
    plan: &SourcingDraftPlan,
    basis_id: &str,
) -> CreatePurchaseOrderFromBasisRequest {
    CreatePurchaseOrderFromBasisRequest {
        work_item_id: input.req.work_item_id.clone(),
        basis_id: basis_id.to_string(),
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
        idempotency_key: input.req.idempotency_key.clone(),
    }
}

/// 完成任务同步后读取最终状态、登记命令收据，并返回同一份实际写入结果。
async fn finish_sourcing(
    db: &Database,
    input: &CreateFromSourcingApplyInput<'_>,
    orders: Vec<CreatePurchaseOrderResult>,
    stock_reservations: Vec<ExistingStockReservationResult>,
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrdersFromSourcingResult> {
    sync_procurement_tasks_for_sales_order(db, input.sales_order_id, executor).await?;
    let status = db
        .work_items()
        .find_by_id(&input.req.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("供给分配任务在同步后不存在".to_string()))?
        .status;
    let response_status = sourcing_work_item_status(status, false)?;
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
        work_item_status: Some(status),
    };
    write_sourcing_receipt(
        db,
        input.audit_id,
        input.request_fingerprint,
        input.sales_order_id.as_ref(),
        &receipt,
        input.actor,
        executor,
    )
    .await?;
    Ok(CreatePurchaseOrdersFromSourcingResult {
        orders,
        stock_reservations,
        work_item_status: response_status,
        replayed: false,
        reference: input.sales_order_id.to_string(),
    })
}
