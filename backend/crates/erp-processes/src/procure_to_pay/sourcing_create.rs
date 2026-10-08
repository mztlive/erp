//! 按供给分配结果一次落地现有库存预占和采购缺口。
//!
//! 操作人确认库存优先的推荐结果后，本模块在同一事务内推进一次供给 guard、
//! 原子预占现有库存并生成仓发草稿，再把剩余采购分配按供应商、采购类型、
//! 付款条件和履约责任拆成采购单并启动审批。

use std::collections::{BTreeMap, HashSet};

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::ids::{DeliveryId, DeliveryLineId, SalesOrderId, WarehouseId};
use erp_fulfillment::entity::fulfillment::{
    Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryType,
};
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::SharedRbacService;
use erp_inventory::StockReservation;
use erp_procurement::dto::purchase_order::{
    CREATE_SOURCING_ACTION, CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest,
    CreatePurchaseOrderResult, CreatePurchaseOrdersFromSourcingRequest,
    CreatePurchaseOrdersFromSourcingResult, ExistingStockReservationResult,
};
use erp_procurement::entity::purchase_order::{
    PurchaseCommandReceipt, PurchaseCommandReceiptError, PurchaseCommandReceiptIdentity,
    SourcingAssignmentSet, SourcingOrderReceipt, SourcingPlan, SourcingPlanError, SourcingReceipt,
    SourcingTaskStatus, StockBasisGroup, basis_id_for,
};
use erp_procurement::repository::PurchaseCommandExt;
use erp_read_models::purchase_center::repository::{sales_order_basis_fact, sourcing_groups_for_order};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemStatus;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::PurchaseOrderProcess;
use super::authorization::{PurchaseOrderAuthorization, ensure_purchase_order_actor_account};
use super::creation_basis::{
    CreateBasisCommand, VerifiedBasisInput, basis_groups_and_facts, load_effective_sales_order,
    persist_basis_draft, procurement_quantity_changed, stock_basis_groups_for_order,
    validate_requested_quantities,
};
use super::procurement_task_sync::{
    load_owned_open_procurement_task, sync_procurement_tasks_for_sales_order,
};
use crate::audit::{persist_log, recover_command};
use crate::{Error, Result};

mod apply;
pub(super) mod sequence;
mod stock_posting;
use apply::create_from_sourcing_apply;
use sequence::{PurchaseCreationEventSequence, SourcingEventSequencePlan};
use stock_posting::{PersistedStockAllocation, persist_stock_allocations};

const CREATE_PERMISSION: &str = "purchase_order:create";
const CREATE_SOURCING_RECEIPT_PREFIX: &str = "purchase-order-sourcing-command-";
const CREATE_SOURCING_ITEM_PREFIX: &str = "purchase-order-sourcing-item-";

/// 原命令入口向授权事务传递的完整输入。
#[derive(Clone, Copy)]
struct SourcingTransactionInput<'a> {
    /// 原请求，克隆时机保持在事务准备阶段。
    req: &'a CreatePurchaseOrdersFromSourcingRequest,
    /// 已解析的来源销售单。
    sales_order_id: &'a SalesOrderId,
    /// 原命令稳定收据身份。
    receipt_identity: &'a PurchaseCommandReceiptIdentity,
    /// 原规范化请求指纹。
    request_fingerprint: &'a str,
    /// 已鉴权操作人。
    actor: &'a AuditActor,
}

impl PurchaseOrderProcess {
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
        )?;
        let authorization = self.authorize_actor_permission(actor, CREATE_PERMISSION).await?;
        let command = SourcingTransactionInput {
            req: &req,
            sales_order_id: &sales_order_id,
            receipt_identity: &receipt_identity,
            request_fingerprint: &request_fingerprint,
            actor,
        };
        if let Some(result) = replay_sourcing_command(&self.db, command, &mut NoTransaction).await? {
            return Ok(result);
        }
        let transaction_result = self.sourcing_transaction(command, assignments, authorization).await;
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => {
                recover_command(error, replay_sourcing_command(&self.db, command, &mut NoTransaction).await)
            },
        }
    }

    /// 保留授权事务的原克隆、账号复验和实际业务写入顺序。
    async fn sourcing_transaction(
        &self,
        input: SourcingTransactionInput<'_>,
        assignments: SourcingAssignmentSet,
        authorization: PurchaseOrderAuthorization,
    ) -> Result<CreatePurchaseOrdersFromSourcingResult> {
        let PurchaseOrderAuthorization { rbac, policy_revision } = authorization;
        let SourcingTransactionInput { req, sales_order_id, receipt_identity, request_fingerprint, actor } =
            input;
        let db = self.db.clone();
        let binding_rbac = rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let transaction_actor = actor.clone();
        let transaction_req = req.clone();
        let transaction_fingerprint = request_fingerprint.to_string();
        let transaction_receipt_identity = receipt_identity.clone();
        let transaction_sales_order_id = sales_order_id.clone();
        rbac.run_authorized_policy_transaction(policy_revision, move |executor| {
            Box::pin(async move {
                ensure_purchase_order_actor_account(&db, &transaction_actor, executor).await?;
                create_from_sourcing_apply(
                    &db,
                    CreateFromSourcingApplyInput {
                        rbac: &binding_rbac,
                        object_read: object_read.as_ref(),
                        req: &transaction_req,
                        assignments: &assignments,
                        sales_order_id: &transaction_sales_order_id,
                        receipt_identity: &transaction_receipt_identity,
                        request_fingerprint: &transaction_fingerprint,
                        actor: &transaction_actor,
                    },
                    executor,
                )
                .await
            })
        })
        .await
    }
}

/// 保持入口与提交结果查证共用相同资源身份和任务引用。
async fn replay_sourcing_command(
    db: &mongodb::Database,
    command: SourcingTransactionInput<'_>,
    executor: &mut dyn Executor,
) -> Result<Option<CreatePurchaseOrdersFromSourcingResult>> {
    replay_sourcing(
        db,
        command.receipt_identity,
        command.request_fingerprint,
        command.actor,
        command.sales_order_id.as_ref(),
        &command.req.work_item_id,
        executor,
    )
    .await
}

/// 选源建单事务内写入所需上下文。
struct CreateFromSourcingApplyInput<'a> {
    /// 审批绑定授权源。
    rbac: &'a SharedRbacService,
    /// 组合根审批对象读取端口。
    object_read: &'a dyn erp_workflow::ApprovalObjectReadPort,
    /// 原始选源请求。
    req: &'a CreatePurchaseOrdersFromSourcingRequest,
    /// 已规范化且稳定行不重复的选源集合。
    assignments: &'a SourcingAssignmentSet,
    /// 来源销售单。
    sales_order_id: &'a SalesOrderId,
    /// 整批命令收据 ID。
    receipt_identity: &'a PurchaseCommandReceiptIdentity,
    /// 整批命令载荷指纹。
    request_fingerprint: &'a str,
    /// 审计操作人。
    actor: &'a AuditActor,
}

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
    groups.iter().find(|group| group.balance.base.id == balance_id).ok_or_else(procurement_quantity_changed)
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
        SourcingPlanError::WarehouseContract(message) | SourcingPlanError::QuantityContract(message) => {
            Error::ValidationError(message)
        },
    }
}

/// 为现有库存预占创建或补充同仓仓发草稿。
async fn create_stock_delivery_drafts(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    allocations: &[PersistedStockAllocation],
    executor: &mut dyn Executor,
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
            executor,
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
    executor: &mut dyn Executor,
) -> Result<()> {
    let existing = db.fulfillment().draft_warehouse_delivery(sales_order_id, warehouse_id, executor).await?;
    if let Some(delivery) = existing {
        append_stock_delivery_lines(db, &delivery, reservations, executor).await?;
        crate::fulfillment_execution::task::ensure_fulfillment_task(
            db,
            crate::fulfillment_execution::task::FulfillmentTaskObject::Delivery(&delivery),
            executor,
        )
        .await?;
        return Ok(());
    }
    let delivery_id = DeliveryId::new(next_id());
    let delivery = Delivery::new(
        delivery_id.clone(),
        DeliveryData {
            delivery_no: erp_fulfillment::service::document_number::next_delivery_no(db).await?,
            delivery_type: DeliveryType::WarehouseShip,
            sales_order_id: sales_order_id.clone(),
            purchase_order_id: None,
            warehouse_id: Some(warehouse_id.clone()),
            tracking_entries: Vec::new(),
            address_snapshot_encrypted: None,
            address_snapshot_fingerprint: None,
        },
    )?;
    let lines = build_stock_delivery_lines(&delivery_id, reservations, 1)?;
    db.fulfillment().create_delivery_with_lines(&delivery, &lines, executor).await?;
    crate::fulfillment_execution::task::ensure_fulfillment_task(
        db,
        crate::fulfillment_execution::task::FulfillmentTaskObject::Delivery(&delivery),
        executor,
    )
    .await
}

/// 向现有仓发草稿追加尚未出现的库存预占行。
async fn append_stock_delivery_lines(
    db: &mongodb::Database,
    delivery: &Delivery,
    reservations: &[&StockReservation],
    executor: &mut dyn Executor,
) -> Result<()> {
    let delivery_id = DeliveryId::new(delivery.base.id.clone());
    let existing =
        db.fulfillment().delivery_lines_by_delivery_ids(std::slice::from_ref(&delivery_id), executor).await?;
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
        db.delivery_lines().create(&line, executor).await?;
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
/// * `input` - 当前稳定命令身份、载荷指纹及操作人
/// * `sales_order` - 原事务读取的来源销售单及真实编号，作为收据资源身份
/// * `receipt` - 采购单与库存预留的完整命令结果
/// * `event_sequence` - 业务首写前验证的主事件序号
/// * `executor` - 数据访问执行器
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
    input: &CreateFromSourcingApplyInput<'_>,
    sales_order: &SalesOrder,
    receipt: &SourcingReceipt,
    event_sequence: u32,
    executor: &mut dyn Executor,
) -> Result<()> {
    let audit = sourcing_audit(
        input.actor,
        input.receipt_identity.receipt_id(),
        &sales_order.base.id,
        &sales_order.order_no,
        event_sequence,
    )?;
    let record = PurchaseCommandReceipt::new(
        input.receipt_identity,
        input.request_fingerprint,
        receipt.clone(),
        audit.base.id.clone(),
    )?;
    db.purchase_command_receipts::<SourcingReceipt>().create(&record, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 投影批次命令的最后主事件，保留发生时销售单编号。
/// # 参数
/// * `actor` - 已鉴权操作人。
/// * `command_id` - 整批稳定命令关联。
/// * `sales_order_id` - 来源销售单 ID。
/// * `sales_order_no` - 来源销售单真实编号。
/// * `event_sequence` - 首次业务写入前验证的主事件序号。
/// # 返回
/// 返回保留安全快照和事件顺序的批次日志。
/// # 错误
/// 审计身份、动作、编号或序号非法时拒绝。
pub(super) fn sourcing_audit(
    actor: &AuditActor,
    command_id: &str,
    sales_order_id: &str,
    sales_order_no: &str,
    event_sequence: u32,
) -> Result<AuditLog> {
    Ok(actor
        .clone()
        .resource_log_with_id(
            next_id(),
            CREATE_SOURCING_ACTION,
            "sales_order",
            sales_order_id.to_string(),
            None,
        )?
        .with_command_id(Some(command_id.to_string()))?
        .with_resource_number(Some(sales_order_no.to_string()))?
        .with_event_sequence(event_sequence)?)
}

/// 查询并校验选源创建幂等收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `receipt_identity` - 稳定收据 ID
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
    receipt_identity: &PurchaseCommandReceiptIdentity,
    expected_fingerprint: &str,
    _actor: &AuditActor,
    sales_order_id: &str,
    _work_item_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<CreatePurchaseOrdersFromSourcingResult>> {
    let Some(record) = db
        .purchase_command_receipts::<SourcingReceipt>()
        .find_by_id_including_deleted(receipt_identity.receipt_id(), executor)
        .await?
    else {
        return Ok(None);
    };
    let receipt = match PurchaseCommandReceipt::<SourcingReceipt>::decode(
        record,
        receipt_identity,
        expected_fingerprint,
    ) {
        Ok(receipt) => receipt,
        Err(PurchaseCommandReceiptError::IdentityMismatch | PurchaseCommandReceiptError::PayloadConflict) => {
            return Err(Error::ConflictError("幂等键已用于不同采购命令".to_string()));
        },
        Err(PurchaseCommandReceiptError::Corrupted(message)) => {
            return Err(Error::Internal(message));
        },
    }
    .into_payload();
    let work_item_status = receipt.work_item_status.as_str().to_string();
    Ok(Some(CreatePurchaseOrdersFromSourcingResult {
        orders: receipt
            .orders
            .into_iter()
            .map(|order| {
                CreatePurchaseOrderResult::new(
                    order.purchase_order_id.clone(),
                    order.purchase_no,
                    order.purchase_order_id,
                )
                .with_lock_version(order.lock_version)
                .with_replayed(true)
            })
            .collect(),
        stock_reservations: receipt.stock_reservations,
        work_item_status,
        replayed: true,
        reference: sales_order_id.to_string(),
    }))
}

/// 将工作流任务状态显式转换为采购拥有的命令结果状态。
fn sourcing_work_item_status(status: WorkItemStatus) -> Result<SourcingTaskStatus> {
    match status {
        WorkItemStatus::Open => Ok(SourcingTaskStatus::Open),
        WorkItemStatus::Completed => Ok(SourcingTaskStatus::Completed),
        WorkItemStatus::Closed => Err(Error::Internal("选源幂等收据中的任务状态非法".to_string())),
    }
}
