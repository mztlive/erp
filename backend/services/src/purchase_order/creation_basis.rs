//! 采购创建依据与按销售当前版本剩余数量建单。
//!
//! 创建依据由销售单当前版本的 `GOODS_SERVICE` 行、当前采购覆盖数量和供应商
//! 当前合格供给共同形成。依据精确到销售当前版本、供应商、采购类型、付款条件与
//! 履约责任；一次依据命令只创建一张采购单，并在同一事务内提交审批。
//!
//! 供给、当前修订、可供投影与供应商结算事实由
//! `database::PurchaseOrderExt::load_creation_basis_facts` 一次批量加载；拆单
//! 维度、稳定身份、产品类型映射、履约选项、成本选择、最大可创建数量与请求行
//! 规范化由 `entities::purchase_order::creation_basis` 领域值对象承担。本模块
//! 只负责当前指针解析、任务归属与 RBAC、合格性筛选（条款有效期、AVAILABLE、
//! 零库存、每供应商稳定选一条）、事务编排与 View 映射。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use chrono::{Datelike, FixedOffset};
use database::{
    AccessControlExt, DocumentRegistryExt, Executor, InventoryExt, NoTransaction, PurchaseOrderExt,
    SalesOrderExt, SupplierExt, WarehouseExt, WorkItemExt,
};
use entities::catalog::ProductKind;
use entities::common::time::{BusinessDate, Instant};
use entities::document_registry::DocumentType;
use entities::ids::{
    PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId, SkuId,
    SupplierAccountId, WarehouseId,
};
use entities::money::{line_amounts, Amount, Quantity, UnitPrice};
use entities::purchase_order::{
    basis_id_for, basis_scope_key, fulfillment_options, maximum_create_quantity,
    purchase_type_from_product_kind, stable_line_id, stock_basis_id_for, supply_cost, BasisGroup, BasisLine,
    BasisScope, CreationBasisFacts, FulfillmentResponsibility, LegacyReceiptIdScheme, LineSupply,
    PurchaseCommandReceipt, PurchaseCommandReceiptError, PurchaseLineType, PurchaseOrder, PurchaseOrderData,
    PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
    PurchaseOrderSubmissionLineData, PurchaseType, RequestedLine, SalesProcurementCoverage,
    SalesProcurementCoverageLine, StockBasisGroup, StockBasisLine, SupplierSnapshot,
};
use entities::sales_order::{CommercialStatus, SalesOrder, SalesOrderRevision};
use entities::supplier::SupplierPaymentTerm;
use entities::supplier_offering::{AvailabilityStatus, SupplierOffering};
use entities::warehouse::WarehouseFulfillmentOperation;
use id_generator::next_id;
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::adapter::{purchase_order_object_readable, purchase_order_responsible_org_id};
use super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::coverage::load_sales_procurement_coverage;
use super::create_submit::submit_created_draft_in_session;
use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreationBasisLineView,
    CreationBasisListParams, CreationBasisView, SupplySourceType, CREATE_ACTION,
};
use super::procurement_task_sync::{
    load_owned_open_procurement_task, sync_procurement_tasks_for_sales_order,
};
use super::shared::zero_amount;
use super::PurchaseOrderService;
use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::audit::AuditActor;
use crate::document_registry::new_registered_document;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

const CREATE_PERMISSION: &str = "purchase_order:create";
const CREATE_RECEIPT_PREFIX: &str = "purchase-order-create-command-";

/// 供应商当前商务资料中的付款条件与经营类目。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SupplierSettlementTerms {
    /// 不含经营类目编码的付款条件代码。
    payment_term_code: String,
    /// 经营类目；未登记时为空。
    business_category: Option<String>,
}

impl SupplierSettlementTerms {
    /// 商务资料缺失时的缺省付款条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `NET-30` 且无经营类目。
    ///
    /// # 错误
    /// 无。
    fn net30() -> Self {
        Self {
            payment_term_code: "NET-30".to_string(),
            business_category: None,
        }
    }
}

/// 已通过事务内最新剩余量校验的采购行。
#[derive(Debug, Clone)]
pub(super) struct SelectedLine {
    /// 当前依据行。
    pub(super) basis: BasisLine,
    /// 本次采购数量。
    pub(super) quantity: Quantity,
    /// 采购确认的预计交付日。
    pub(super) expected_delivery_date: BusinessDate,
}

/// 幂等命令收据载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CreationReceipt {
    /// 采购单主键。
    purchase_order_id: String,
    /// 采购单号。
    purchase_no: String,
    /// 创建完成时乐观锁版本。
    lock_version: u64,
}

/// 事务内采购创建命令上下文。
pub(super) struct CreateBasisCommand<'a> {
    /// 来源销售单。
    pub(super) sales_order_id: &'a SalesOrderId,
    /// 原始创建请求。
    pub(super) req: &'a CreatePurchaseOrderFromBasisRequest,
    /// 已规范化逐行数量。
    pub(super) requested_lines: &'a [RequestedLine],
    /// 稳定命令收据 ID。
    pub(super) audit_id: &'a str,
    /// 命令载荷指纹。
    pub(super) request_fingerprint: &'a str,
    /// 审计操作人。
    pub(super) actor: &'a AuditActor,
}

/// 待一次性持久化的采购草稿聚合。
struct PreparedDraftWrite<'a> {
    /// 来源销售单。
    sales_order: &'a SalesOrder,
    /// 新采购单。
    order: &'a PurchaseOrder,
    /// 当前草稿提交。
    submission: &'a PurchaseOrderSubmission,
    /// 当前草稿提交行。
    lines: &'a [PurchaseOrderSubmissionLine],
    /// 审计操作人。
    actor: &'a AuditActor,
}

impl PurchaseOrderService {
    /// 查询当前账号开放采购任务范围内仍有剩余量的精确采购创建依据。
    ///
    /// # 参数
    /// * `params` - 可选销售单与供给分配任务筛选
    /// * `actor` - 当前已认证账号
    ///
    /// # 返回
    /// 返回按具体开放任务、销售单和精确拆分维度形成的创建依据。
    ///
    /// # 错误
    /// 当前销售版本、采购覆盖、任务范围或供应商供给数据不一致，以及仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只展示当前账号拥有的开放任务冻结行；客户端不能看到或创建其他采购负责人的范围。
    /// 供给、修订、可供投影与供应商结算事实按全部任务涉及 SKU 一次批量读取，
    /// 查询次数不随任务数、销售行数或供给数线性增长。
    pub async fn creation_basis_list(
        &self,
        params: &CreationBasisListParams,
        actor: &AuditActor,
    ) -> Result<Vec<CreationBasisView>> {
        let sales_order_id = normalized_optional_filter(params.sales_order_id.as_deref());
        let work_item_id = normalized_optional_filter(params.work_item_id.as_deref());
        let tasks = self
            .db
            .work_items()
            .list_open_procurement_owned_by(
                actor.id(),
                sales_order_id.as_deref(),
                work_item_id.as_deref(),
                &mut NoTransaction,
            )
            .await?;
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let order_ids = tasks
            .iter()
            .map(|task| SalesOrderId::new(task.business_object_id.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let orders = self
            .db
            .purchase_order()
            .find_effective_sales_orders_by_ids(&order_ids, &mut NoTransaction)
            .await?;
        let owner_names = self
            .resolve_account_names(
                &orders
                    .iter()
                    .map(|order| order.stable.created_by.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;
        let orders = orders
            .into_iter()
            .map(|order| (order.base.id.clone(), order))
            .collect::<HashMap<_, _>>();
        // 按销售单归组任务，使覆盖与供给事实按单一次批量读取。
        let mut task_indexes_by_order: Vec<(String, Vec<usize>)> = Vec::new();
        for (index, task) in tasks.iter().enumerate() {
            if task.responsibility_key().is_none() || task.responsibility_scope_ids().is_empty() {
                return Err(Error::ConflictError("供给分配任务缺少冻结责任范围".to_string()));
            }
            if let Some(entry) = task_indexes_by_order
                .iter_mut()
                .find(|(order_id, _)| *order_id == task.business_object_id)
            {
                entry.1.push(index);
            } else {
                task_indexes_by_order.push((task.business_object_id.clone(), vec![index]));
            }
        }
        // 每张销售单一次覆盖读取；全部任务共享同一批供给与结算事实。
        let mut coverage_by_order: HashMap<String, SalesProcurementCoverage> = HashMap::new();
        let mut all_sku_ids: Vec<SkuId> = Vec::new();
        for (order_id, task_indexes) in &task_indexes_by_order {
            let Some(order) = orders.get(order_id) else {
                continue;
            };
            let coverage = load_sales_procurement_coverage(&self.db, order, &mut NoTransaction).await?;
            for &task_index in task_indexes {
                let scope = tasks[task_index]
                    .responsibility_scope_ids()
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>();
                for line in &coverage.lines {
                    if scope.contains(line.revision_line.sales_order_line_id.as_ref())
                        && line.summary.remaining_quantity > zero_quantity()
                    {
                        all_sku_ids.push(line.goods_line.sku_id.clone());
                    }
                }
            }
            coverage_by_order.insert(order_id.clone(), coverage);
        }
        let facts = self
            .db
            .load_creation_basis_facts(&all_sku_ids, &mut NoTransaction)
            .await?;
        let mut views = Vec::new();
        for (order_id, task_indexes) in task_indexes_by_order {
            let Some(order) = orders.get(&order_id) else {
                continue;
            };
            let coverage = coverage_by_order
                .get(&order_id)
                .expect("已加载的销售覆盖必须存在");
            let owner_name = owner_names.get(&order.stable.created_by).cloned();
            for task_index in task_indexes {
                let task = &tasks[task_index];
                let groups =
                    basis_groups_from_facts(order, coverage, task.responsibility_scope_ids(), &facts)?;
                for group in groups {
                    views.push(build_basis_view(
                        order,
                        &group,
                        &facts,
                        owner_name.clone(),
                        &task.base.id,
                    )?);
                }
                let stock_groups = stock_basis_groups_for_order(
                    &self.db,
                    order,
                    task.responsibility_scope_ids(),
                    &mut NoTransaction,
                )
                .await?;
                for group in stock_groups {
                    views.push(build_stock_basis_view(
                        order,
                        &group,
                        owner_name.clone(),
                        &task.base.id,
                    )?);
                }
            }
        }
        Ok(views)
    }

    /// 依据精确拆分维度和逐行本次数量创建一张采购单并提交审批。
    ///
    /// # 参数
    /// * `req` - 精确依据、逐行数量与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已提交审批的采购单；同一幂等键与同一载荷重复提交时返回原结果。
    ///
    /// # 错误
    /// 操作账号不可登录或缺少采购创建权限、依据失效、数量非正或超过事务内最新
    /// 剩余/可供量、幂等键载荷冲突、并发冲突、审批绑定、启动审批或仓储写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 操作人授权版本通过 policy CAS 与提交绑定；事务内再以销售单 CAS guard 串行化并重算剩余量。
    /// 创建成功即进入审批中，不得留下可编辑草稿。
    pub async fn create_from_basis(
        &self,
        req: CreatePurchaseOrderFromBasisRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        req.validate()?;
        let requested_lines = req.normalized_lines()?;
        let request_fingerprint = req.request_fingerprint(&requested_lines);
        let receipt_identity = PurchaseCommandReceipt::<CreationReceipt>::identity(
            CREATE_RECEIPT_PREFIX,
            actor.id(),
            CREATE_ACTION,
            None,
            &req.idempotency_key,
            LegacyReceiptIdScheme::None,
        )?;
        let audit_id = receipt_identity.receipt_id().to_string();
        let PurchaseOrderAuthorization {
            rbac,
            policy_revision,
        } = self.authorize_actor_permission(actor, CREATE_PERMISSION).await?;
        if let Some(result) = replay_creation(
            &self.db,
            &audit_id,
            &request_fingerprint,
            actor,
            &mut NoTransaction,
        )
        .await?
        {
            return Ok(result);
        }
        let sales_order_id = parse_basis_sales_order_id(&req.basis_id)?;
        let db = self.db.clone();
        let binding_rbac = rbac.clone();
        let transaction_actor = actor.clone();
        let transaction_req = req.clone();
        let transaction_fingerprint = request_fingerprint.clone();
        let transaction_audit_id = audit_id.clone();
        let transaction_result = rbac
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    ensure_purchase_order_actor_account(&db, &transaction_actor, session).await?;
                    let command = CreateBasisCommand {
                        sales_order_id: &sales_order_id,
                        req: &transaction_req,
                        requested_lines: &requested_lines,
                        audit_id: &transaction_audit_id,
                        request_fingerprint: &transaction_fingerprint,
                        actor: &transaction_actor,
                    };
                    create_from_basis_in_transaction(&db, &binding_rbac, &command, session).await
                })
            })
            .await;
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => replay_creation(
                &self.db,
                &audit_id,
                &request_fingerprint,
                actor,
                &mut NoTransaction,
            )
            .await?
            .ok_or(error),
        }
    }
}

/// 在 MongoDB 事务内串行化、重算并写入一张采购单后立即提交审批。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `command` - 来源销售单、请求、幂等收据与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回本次创建或事务内命中的幂等结果。
///
/// # 错误
/// 依据、数量、并发 guard、审批绑定或持久化失败时返回错误。
///
/// # 关键业务约束
/// guard CAS 成功后必须再次按采购当前指针计算剩余量，并复用同一事务事实。
async fn create_from_basis_in_transaction(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    command: &CreateBasisCommand<'_>,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrderResult> {
    if let Some(result) = replay_creation(
        db,
        command.audit_id,
        command.request_fingerprint,
        command.actor,
        session,
    )
    .await?
    {
        return Ok(result);
    }
    let task = load_owned_open_procurement_task(
        db,
        &command.req.work_item_id,
        command.sales_order_id,
        command.actor.id(),
        session,
    )
    .await?;
    let mut order = load_effective_sales_order(db, command.sales_order_id, session).await?;
    let groups = basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    let selected =
        find_requested_group(&order, &groups, &command.req.basis_id, &command.req.work_item_id)?.clone();
    ensure_request_scope(command.req, &selected.scope)?;
    order.advance_procurement_guard(command.actor.id())?;
    db.sales_orders().update(&mut order, session).await?;
    let (latest_groups, latest_facts) =
        basis_groups_and_facts(db, &order, task.responsibility_scope_ids(), session).await?;
    let latest = latest_groups
        .into_iter()
        .find(|group| group.scope == selected.scope)
        .ok_or_else(procurement_quantity_changed)?;
    let selected_lines = validate_requested_quantities(command.requested_lines, &latest)?;
    let input = VerifiedBasisInput {
        sales_order: &order,
        group: &latest,
        selected_lines: &selected_lines,
        facts: &latest_facts,
    };
    persist_basis_draft(db, rbac, &input, command, session).await
}

/// 加载可作为采购来源的已生效销售单。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `sales_order_id` - 销售单主键
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回已生效销售单。
///
/// # 错误
/// 销售单不存在、未生效或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 非生效销售单不能占用采购剩余量。
pub(super) async fn load_effective_sales_order(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<SalesOrder> {
    let order = db
        .sales_orders()
        .find_by_id(sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    if order.commercial_status != CommercialStatus::Effective {
        return Err(Error::NotFound("销售单未生效，不能作为采购创建依据".to_string()));
    }
    Ok(order)
}

/// 由销售当前版本、当前覆盖和批量供给事实形成精确依据集合。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已生效销售单
/// * `responsibility_scope_ids` - 当前采购任务冻结的稳定销售行 ID
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回任务责任范围内按精确拆分维度分组并稳定排序的依据。
///
/// # 错误
/// 当前指针、覆盖、供给或付款条件查询失败时返回错误。
///
/// # 关键业务约束
/// 仅对任务冻结范围内的稳定销售行查询供应商供给；同一依据内供应商、采购类型、
/// 付款条件和履约责任完全一致。
pub(super) async fn basis_groups_for_order(
    db: &mongodb::Database,
    order: &SalesOrder,
    responsibility_scope_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<BasisGroup>> {
    Ok(
        basis_groups_and_facts(db, order, responsibility_scope_ids, executor)
            .await?
            .0,
    )
}

/// 由销售当前版本、当前覆盖和批量供给事实形成精确依据集合，并返回本次事实。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已生效销售单
/// * `responsibility_scope_ids` - 当前采购任务冻结的稳定销售行 ID
/// * `executor` - 数据访问执行器；事务内重验必须复用调用方 executor
///
/// # 返回
/// 返回任务责任范围内的依据集合及本次批量加载的供给事实；事实用于事务内
/// 名称快照，避免创建路径再次逐段读取。
///
/// # 错误
/// 当前指针、覆盖、供给或付款条件查询失败时返回错误。
///
/// # 关键业务约束
/// 供给事实查询次数与销售行数、供给数无关；非生效销售单直接返回空集合与空事实。
pub(super) async fn basis_groups_and_facts(
    db: &mongodb::Database,
    order: &SalesOrder,
    responsibility_scope_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<(Vec<BasisGroup>, CreationBasisFacts)> {
    if order.commercial_status != CommercialStatus::Effective {
        return Ok((Vec::new(), CreationBasisFacts::default()));
    }
    let coverage = load_sales_procurement_coverage(db, order, executor).await?;
    let facts = creation_basis_facts_for_order(db, &coverage, responsibility_scope_ids, executor).await?;
    let groups = basis_groups_from_facts(order, &coverage, responsibility_scope_ids, &facts)?;
    Ok((groups, facts))
}

/// 批量加载任务责任范围内销售目标行的供给与供应商结算事实。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `coverage` - 当前销售版本采购覆盖
/// * `responsibility_scope_ids` - 当前采购任务冻结的稳定销售行 ID
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回涉及 SKU 的 ACTIVE 供给、当前修订、可供投影、供应商结算事实与法定名称。
///
/// # 错误
/// 仓储批量读取失败时返回错误。
///
/// # 关键业务约束
/// 只收集任务冻结范围内仍有剩余量的目标行 SKU；查询次数与任务数、销售行数及
/// 供给数无关。
async fn creation_basis_facts_for_order(
    db: &mongodb::Database,
    coverage: &SalesProcurementCoverage,
    responsibility_scope_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<CreationBasisFacts> {
    let scope = responsibility_scope_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let sku_ids = coverage
        .lines
        .iter()
        .filter(|line| {
            scope.contains(line.revision_line.sales_order_line_id.as_ref())
                && line.summary.remaining_quantity > zero_quantity()
        })
        .map(|line| line.goods_line.sku_id.clone())
        .collect::<Vec<_>>();
    db.load_creation_basis_facts(&sku_ids, executor)
        .await
        .map_err(Into::into)
}

/// 由销售当前版本、当前覆盖和批量供给事实形成精确依据集合（纯规则）。
///
/// # 参数
/// * `order` - 已生效销售单
/// * `coverage` - 当前销售版本采购覆盖
/// * `responsibility_scope_ids` - 当前采购任务冻结的稳定销售行 ID
/// * `facts` - 任务涉及 SKU 的批量供给事实
///
/// # 返回
/// 返回任务责任范围内按精确拆分维度分组并稳定排序的依据。
///
/// # 错误
/// 商品类型映射或可供数量非法时返回错误。
///
/// # 关键业务约束
/// 同一依据内供应商、采购类型、付款条件和履约责任完全一致；非生效销售单返回
/// 空集合；`min(remaining, available)` 为零时丢弃该供应商。
fn basis_groups_from_facts(
    order: &SalesOrder,
    coverage: &SalesProcurementCoverage,
    responsibility_scope_ids: &[String],
    facts: &CreationBasisFacts,
) -> Result<Vec<BasisGroup>> {
    if order.commercial_status != CommercialStatus::Effective {
        return Ok(Vec::new());
    }
    let responsibility_scope_ids = responsibility_scope_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut groups: Vec<BasisGroup> = Vec::new();
    for line in &coverage.lines {
        if !responsibility_scope_ids.contains(line.revision_line.sales_order_line_id.as_ref())
            || line.summary.remaining_quantity <= zero_quantity()
        {
            continue;
        }
        let supplies = qualified_supplies_for_line(facts, line)?;
        append_line_supplies(&coverage.revision, line.clone(), supplies, facts, &mut groups)?;
    }
    for group in &mut groups {
        group
            .lines
            .sort_by(|left, right| stable_line_id(left).cmp(stable_line_id(right)));
    }
    groups.sort_by_key(|group| basis_scope_key(&group.scope));
    Ok(groups)
}

/// 由销售当前版本、统一覆盖与公司可用库存形成现有库存供给依据。
pub(super) async fn stock_basis_groups_for_order(
    db: &mongodb::Database,
    order: &SalesOrder,
    responsibility_scope_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<StockBasisGroup>> {
    if order.commercial_status != CommercialStatus::Effective {
        return Ok(Vec::new());
    }
    let scope = responsibility_scope_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let coverage = load_sales_procurement_coverage(db, order, executor).await?;
    let physical_lines = coverage
        .lines
        .iter()
        .filter(|line| {
            line.product_kind == ProductKind::Physical
                && scope.contains(line.revision_line.sales_order_line_id.as_ref())
                && line.summary.remaining_quantity > zero_quantity()
        })
        .cloned()
        .collect::<Vec<_>>();
    let sku_ids = physical_lines
        .iter()
        .map(|line| line.goods_line.sku_id.clone())
        .collect::<Vec<_>>();
    let balances = db
        .inventory()
        .available_balances_for_skus(&sku_ids, executor)
        .await?;
    let warehouse_ids = balances
        .iter()
        .map(|balance| balance.warehouse_id.to_string())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let warehouses = db.inventory().warehouses_by_ids(&warehouse_ids, executor).await?;
    let active_warehouses = warehouses
        .into_iter()
        .filter(|warehouse| warehouse.is_active())
        .collect::<Vec<_>>();
    let active_warehouse_ids = active_warehouses
        .iter()
        .map(|warehouse| warehouse.base.id.clone())
        .collect::<HashSet<_>>();
    let revision_ids = active_warehouses
        .iter()
        .filter_map(|warehouse| warehouse.stable.current_revision_id.clone())
        .collect::<Vec<_>>();
    let revisions = db
        .inventory()
        .warehouse_revisions_by_ids(&revision_ids, executor)
        .await?;
    let names = active_warehouses
        .into_iter()
        .map(|warehouse| {
            let name = warehouse
                .stable
                .current_revision_id
                .as_deref()
                .and_then(|revision_id| revisions.iter().find(|revision| revision.base.id == revision_id))
                .map(|revision| revision.name.clone())
                .unwrap_or_else(|| warehouse.base.id.clone());
            (warehouse.base.id, name)
        })
        .collect::<HashMap<_, _>>();
    let mut groups = balances
        .into_iter()
        .filter(|balance| active_warehouse_ids.contains(balance.warehouse_id.as_ref()))
        .filter_map(|balance| {
            let lines = physical_lines
                .iter()
                .filter(|line| line.goods_line.sku_id == balance.sku_id)
                .cloned()
                .map(|coverage| StockBasisLine {
                    max_create_quantity: coverage
                        .summary
                        .remaining_quantity
                        .min(balance.available_quantity),
                    coverage,
                })
                .collect::<Vec<_>>();
            (!lines.is_empty()).then(|| StockBasisGroup {
                warehouse_name: names
                    .get(balance.warehouse_id.as_ref())
                    .cloned()
                    .unwrap_or_else(|| balance.warehouse_id.to_string()),
                revision: coverage.revision.clone(),
                balance,
                lines,
            })
        })
        .collect::<Vec<_>>();
    for group in &mut groups {
        group.lines.sort_by(|left, right| {
            left.coverage
                .revision_line
                .sales_order_line_id
                .cmp(&right.coverage.revision_line.sales_order_line_id)
        });
    }
    groups.sort_by(|left, right| left.balance.base.id.cmp(&right.balance.base.id));
    Ok(groups)
}

/// 将可选查询文本规范化为空或去除首尾空白后的值。
///
/// # 参数
/// * `value` - 可选原始查询文本
///
/// # 返回
/// 空白值返回 `None`，否则返回规范化字符串。
///
/// # 错误
/// 无。
fn normalized_optional_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 将一条销售目标行的合格供给加入精确依据分组。
///
/// # 参数
/// * `revision` - 销售当前版本
/// * `line` - 当前销售版本目标行
/// * `supplies` - 每供应商一条确定供给
/// * `facts` - 批量供给与供应商结算事实
/// * `groups` - 待追加依据集合
///
/// # 返回
/// 追加完成返回 `Ok(())`。
///
/// # 错误
/// 商品类型映射或可供数量计算失败时返回错误。
///
/// # 关键业务约束
/// 有限可供量使用 `min(remaining, available)`，不因不足全量而丢弃供应商；付款
/// 条件与经营类目只从批量事实解释，不再逐供应商读取。
fn append_line_supplies(
    revision: &SalesOrderRevision,
    line: SalesProcurementCoverageLine,
    supplies: Vec<LineSupply>,
    facts: &CreationBasisFacts,
    groups: &mut Vec<BasisGroup>,
) -> Result<()> {
    for supply in supplies {
        let supplier_id = supply.offering.supplier_id.clone();
        let terms = settlement_terms_for(facts, &supplier_id);
        let purchase_type = purchase_type_from_product_kind(line.product_kind)?;
        let max_create_quantity = maximum_create_quantity(
            line.summary.remaining_quantity,
            supply.availability.available_quantity,
        )?;
        if max_create_quantity <= zero_quantity() {
            continue;
        }
        for &fulfillment_responsibility in fulfillment_options(line.product_kind)? {
            let scope = BasisScope {
                supplier_id: supplier_id.clone(),
                purchase_type,
                payment_term_code: terms.payment_term_code.clone(),
                fulfillment_responsibility,
            };
            let basis_line = BasisLine {
                coverage: line.clone(),
                supply: supply.clone(),
                max_create_quantity,
            };
            if let Some(group) = groups.iter_mut().find(|group| group.scope == scope) {
                group.lines.push(basis_line);
            } else {
                groups.push(BasisGroup {
                    revision: revision.clone(),
                    scope,
                    business_category: terms.business_category.clone(),
                    lines: vec![basis_line],
                });
            }
        }
    }
    Ok(())
}

/// 从批量事实解释供应商当前付款条件与经营类目。
///
/// # 参数
/// * `facts` - 批量供应商结算事实
/// * `supplier_id` - 供应商身份
///
/// # 返回
/// 返回该供应商已拆开的付款条件与经营类目；供应商、商务版本缺失或付款条件为
/// 空时付款条件回退 `NET-30`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 付款条件是精确拆单维度的一部分；经营类目不得写入付款条件代码。
fn settlement_terms_for(
    facts: &CreationBasisFacts,
    supplier_id: &SupplierAccountId,
) -> SupplierSettlementTerms {
    let Some(supplier) = facts.suppliers.get(&supplier_id.to_string()) else {
        return SupplierSettlementTerms::net30();
    };
    let Some(revision_id) = supplier.current_commercial_profile_revision_id.clone() else {
        return SupplierSettlementTerms::net30();
    };
    let Some(revision) = facts.commercial_profiles.get(&revision_id.to_string()) else {
        return SupplierSettlementTerms::net30();
    };
    let payment_term_code = revision.effective_payment_term_code();
    SupplierSettlementTerms {
        payment_term_code: if payment_term_code.is_empty() {
            "NET-30".to_string()
        } else {
            payment_term_code
        },
        business_category: revision.effective_business_category(),
    }
}

/// 查询一条销售当前版本行的合格供给，并为每个供应商确定一条稳定供给。
///
/// # 参数
/// * `facts` - 批量 ACTIVE 供给、当前修订与可供投影
/// * `line` - 销售当前版本目标行
///
/// # 返回
/// 返回按供应商和供给 ID 稳定排序、每供应商最多一条的供给。
///
/// # 错误
/// 可供数量为负时返回业务错误。
///
/// # 关键业务约束
/// 仅 ACTIVE、条款当前有效且 availability 为 AVAILABLE 的供给合格；同一 SKU
/// 的供给顺序由批量事实保证，与逐 SKU 查询完全一致。
fn qualified_supplies_for_line(
    facts: &CreationBasisFacts,
    line: &SalesProcurementCoverageLine,
) -> Result<Vec<LineSupply>> {
    let mut seen_suppliers = HashSet::new();
    let mut supplies = Vec::new();
    for offering in facts
        .offerings
        .iter()
        .filter(|offering| offering.sku_id == line.goods_line.sku_id)
    {
        if seen_suppliers.contains(&offering.supplier_id.to_string()) {
            continue;
        }
        let Some(supply) = qualified_supply(facts, offering)? else {
            continue;
        };
        seen_suppliers.insert(supply.offering.supplier_id.to_string());
        supplies.push(supply);
    }
    Ok(supplies)
}

/// 复验单条供给当前修订与可供投影。
///
/// # 参数
/// * `facts` - 批量供给修订与可供投影
/// * `offering` - ACTIVE 供给稳定身份
///
/// # 返回
/// 当前合格时返回供给；缺少当前修订、条款失效或不可供时返回 `None`。
///
/// # 错误
/// 可供数量为负时返回业务错误。
///
/// # 关键业务约束
/// 可供数量为空表示供应商未给出上限，不等于不可供；条款有效期按当前业务日期
/// 判定，业务日期由 Service 注入。
fn qualified_supply(facts: &CreationBasisFacts, offering: &SupplierOffering) -> Result<Option<LineSupply>> {
    let Some(revision_id) = offering.stable.current_revision_id.clone() else {
        return Ok(None);
    };
    let Some(revision) = facts.revisions.get(&revision_id) else {
        return Ok(None);
    };
    let today = BusinessDate::today();
    if revision.valid_from > today || revision.valid_to.is_some_and(|valid_to| valid_to < today) {
        return Ok(None);
    }
    let Some(availability) = facts.availabilities.get(&offering.base.id.to_string()) else {
        return Ok(None);
    };
    if availability.availability_status != AvailabilityStatus::Available {
        return Ok(None);
    }
    if availability
        .available_quantity
        .is_some_and(|quantity| quantity < zero_quantity())
    {
        return Err(Error::BusinessLogicError("供应商可供数量不能为负".to_string()));
    }
    if availability
        .available_quantity
        .is_some_and(|quantity| quantity == zero_quantity())
    {
        return Ok(None);
    }
    Ok(Some(LineSupply {
        offering: offering.clone(),
        revision: revision.clone(),
        availability: availability.clone(),
    }))
}

/// 构造一条精确创建依据视图。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `group` - 精确依据分组
/// * `facts` - 批量供应商结算事实
/// * `sales_owner_name` - 销售负责人展示名
/// * `work_item_id` - 冻结本依据责任范围的开放供给分配任务
///
/// # 返回
/// 返回前端可直接选择逐行数量的依据视图。
///
/// # 错误
/// 履约期限无法转换为业务日期时返回错误。
///
/// # 关键业务约束
/// 预计金额按 `max_create_quantity` 逐行舍入后汇总；供应商名称只从批量事实读取。
fn build_basis_view(
    order: &SalesOrder,
    group: &BasisGroup,
    facts: &CreationBasisFacts,
    sales_owner_name: Option<String>,
    work_item_id: &str,
) -> Result<CreationBasisView> {
    let supplier_name = facts
        .supplier_names
        .get(&group.scope.supplier_id.to_string())
        .cloned()
        .unwrap_or_else(|| group.scope.supplier_id.to_string());
    let mut estimated = zero_amount();
    let mut lines = Vec::with_capacity(group.lines.len());
    for line in &group.lines {
        let cost = supply_cost(&line.supply.revision, group.scope.fulfillment_responsibility);
        let (gross, _, _) = line_amounts(
            cost,
            line.max_create_quantity,
            line.supply.revision.input_tax_rate,
        );
        estimated = estimated.checked_add(gross);
        lines.push(basis_line_view(line, &group.scope.supplier_id, cost, gross)?);
    }
    Ok(CreationBasisView {
        work_item_id: work_item_id.to_string(),
        basis_id: basis_id_for(order, group, work_item_id, None),
        source_type: SupplySourceType::Purchase,
        sales_order_id: order.base.id.clone(),
        sales_order_no: order.order_no.clone(),
        customer_name: group.revision.customer_snapshot.customer_name.clone(),
        contract_no: group
            .revision
            .contract_snapshot
            .as_ref()
            .map(|snapshot| snapshot.contract_no.clone()),
        sales_owner_name,
        sales_order_revision_id: group.revision.base.id.clone(),
        supplier_id: group.scope.supplier_id.to_string(),
        supplier_name,
        stock_balance_id: None,
        warehouse_id: None,
        warehouse_name: None,
        source_available_quantity: None,
        purchase_type: group.scope.purchase_type.as_str().to_string(),
        fulfillment_responsibility: group.scope.fulfillment_responsibility.as_str().to_string(),
        payment_term_code: group.scope.payment_term_code.clone(),
        business_category: group.business_category.clone(),
        lines,
        estimated_gross: estimated.to_string(),
    })
}

/// 构造一个现有库存供给依据视图。
fn build_stock_basis_view(
    order: &SalesOrder,
    group: &StockBasisGroup,
    sales_owner_name: Option<String>,
    work_item_id: &str,
) -> Result<CreationBasisView> {
    let mut lines = Vec::with_capacity(group.lines.len());
    for line in &group.lines {
        let sales_delivery_deadline =
            business_date_of(line.coverage.goods_line.fulfillment_due_at)?.to_string();
        lines.push(CreationBasisLineView {
            sales_order_line_id: line.coverage.revision_line.sales_order_line_id.to_string(),
            sales_order_revision_line_id: line.coverage.revision_line.base.id.clone(),
            sales_line_no: line.coverage.revision_line.line_no,
            supplier_id: String::new(),
            sales_quantity: line.coverage.summary.total_quantity.to_string(),
            covered_quantity: line.coverage.summary.covered_quantity.to_string(),
            remaining_quantity: line.coverage.summary.remaining_quantity.to_string(),
            max_create_quantity: line.max_create_quantity.to_string(),
            confirmed_quantity: line.max_create_quantity.to_string(),
            latest_cost_gross: "0".to_string(),
            input_tax_rate: "0".to_string(),
            expected_delivery_date: sales_delivery_deadline.clone(),
            sales_delivery_deadline,
            product_name: Some(line.coverage.revision_line.item_name_snapshot.clone()),
            specification: line.coverage.revision_line.spec_snapshot.clone(),
            unit: line.coverage.revision_line.unit_snapshot.clone(),
            gross_amount: "0".to_string(),
        });
    }
    Ok(CreationBasisView {
        work_item_id: work_item_id.to_string(),
        basis_id: stock_basis_id_for(order, group, work_item_id),
        source_type: SupplySourceType::ExistingStock,
        sales_order_id: order.base.id.clone(),
        sales_order_no: order.order_no.clone(),
        customer_name: group.revision.customer_snapshot.customer_name.clone(),
        contract_no: group
            .revision
            .contract_snapshot
            .as_ref()
            .map(|snapshot| snapshot.contract_no.clone()),
        sales_owner_name,
        sales_order_revision_id: group.revision.base.id.clone(),
        supplier_id: String::new(),
        supplier_name: format!("现有库存 · {}", group.warehouse_name),
        stock_balance_id: Some(group.balance.base.id.clone()),
        warehouse_id: Some(group.balance.warehouse_id.to_string()),
        warehouse_name: Some(group.warehouse_name.clone()),
        source_available_quantity: Some(group.balance.available_quantity.to_string()),
        purchase_type: PurchaseType::Physical.as_str().to_string(),
        fulfillment_responsibility: FulfillmentResponsibility::Warehouse.as_str().to_string(),
        payment_term_code: String::new(),
        business_category: None,
        lines,
        estimated_gross: "0".to_string(),
    })
}

/// 构造单条依据行视图。
///
/// # 参数
/// * `line` - 精确依据行
/// * `supplier_id` - 当前依据供应商
/// * `cost` - 当前供给含税成本
/// * `gross` - 按最大可创建数量计算的含税行金额
///
/// # 返回
/// 返回销售目标、覆盖、剩余与可创建数量视图。
///
/// # 错误
/// 履约期限无法转换为业务日期时返回错误。
///
/// # 关键业务约束
/// 稳定销售行和当前销售版本行同时下发。
fn basis_line_view(
    line: &BasisLine,
    supplier_id: &SupplierAccountId,
    cost: UnitPrice,
    gross: Amount,
) -> Result<CreationBasisLineView> {
    let sales_delivery_deadline = business_date_of(line.coverage.goods_line.fulfillment_due_at)?.to_string();
    Ok(CreationBasisLineView {
        sales_order_line_id: stable_line_id(line).to_string(),
        sales_order_revision_line_id: line.coverage.revision_line.base.id.clone(),
        sales_line_no: line.coverage.revision_line.line_no,
        supplier_id: supplier_id.to_string(),
        sales_quantity: line.coverage.summary.total_quantity.to_string(),
        covered_quantity: line.coverage.summary.covered_quantity.to_string(),
        remaining_quantity: line.coverage.summary.remaining_quantity.to_string(),
        max_create_quantity: line.max_create_quantity.to_string(),
        confirmed_quantity: line.max_create_quantity.to_string(),
        latest_cost_gross: cost.to_string(),
        input_tax_rate: line.supply.revision.input_tax_rate.to_string(),
        expected_delivery_date: sales_delivery_deadline.clone(),
        sales_delivery_deadline,
        product_name: Some(line.coverage.revision_line.item_name_snapshot.clone()),
        specification: line.coverage.revision_line.spec_snapshot.clone(),
        unit: line.coverage.revision_line.unit_snapshot.clone(),
        gross_amount: gross.to_string(),
    })
}

/// guard 重算后的事务内创建输入：已完成 CAS 的销售单、最新依据范围、
/// 校验通过的本次采购行与批量加载的供给及供应商结算事实。
///
/// # 参数
/// * `sales_order` - 已完成 guard CAS 的销售单
/// * `group` - guard 后重算得到的最新依据范围
/// * `selected_lines` - 事务内校验通过的本次采购行
/// * `facts` - guard 后重算时批量加载的供给与供应商结算事实
pub(super) struct VerifiedBasisInput<'a> {
    /// 已完成 guard CAS 的销售单。
    pub(super) sales_order: &'a SalesOrder,
    /// guard 后重算得到的最新依据范围。
    pub(super) group: &'a BasisGroup,
    /// 事务内校验通过的本次采购行。
    pub(super) selected_lines: &'a [SelectedLine],
    /// guard 后重算时批量加载的供给与供应商结算事实。
    pub(super) facts: &'a CreationBasisFacts,
}

/// 在事务内持久化一张精确依据采购单并提交审批。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `input` - guard 重算后的事务内创建输入（销售单、依据范围、本次行与事实）
/// * `command` - 原始请求、命令收据与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回已提交审批的创建结果。
///
/// # 错误
/// 实体构造、审批绑定、启动审批或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// `creation_basis_id` 唯一，且本函数只创建一个采购聚合；命令收据记录提交后正式号。
/// 供应商名称快照只从同一事务内批量加载的事实读取，不得再次逐段查询。
pub(super) async fn persist_basis_draft(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    input: &VerifiedBasisInput<'_>,
    command: &CreateBasisCommand<'_>,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrderResult> {
    let sales_order = input.sales_order;
    let group = input.group;
    let selected_lines = input.selected_lines;
    let facts = input.facts;
    let target_warehouse_id = resolve_target_warehouse(
        db,
        rbac,
        group.scope.fulfillment_responsibility,
        command.req.target_warehouse_id.as_deref(),
        session,
    )
    .await?;
    ensure_initial_purchase_order_owner(
        db,
        rbac,
        group.scope.fulfillment_responsibility,
        command.actor.id(),
        session,
    )
    .await?;
    let creation_basis_id = basis_id_for(
        sales_order,
        group,
        &command.req.work_item_id,
        target_warehouse_id.as_ref(),
    );
    let order_id = PurchaseOrderId::new(next_id());
    let mut order = PurchaseOrder::new(
        order_id.clone(),
        PurchaseOrderData {
            purchase_no: String::new(),
            sales_order_id: SalesOrderId::new(sales_order.base.id.clone()),
            sales_order_revision_id: group.revision.base.id.clone().into(),
            creation_basis_id,
            supplier_id: group.scope.supplier_id.clone(),
            purchase_type: group.scope.purchase_type,
            payment_term_code: group.scope.payment_term_code.clone(),
            fulfillment_responsibility: group.scope.fulfillment_responsibility,
            owner_user_id: command.actor.id().to_string(),
            target_warehouse_id,
        },
        command.actor.id(),
    )?;
    let supplier_name = facts
        .supplier_names
        .get(&group.scope.supplier_id.to_string())
        .cloned()
        .unwrap_or_else(|| group.scope.supplier_id.to_string());
    let computed = compute_selected_lines(selected_lines, group.scope.fulfillment_responsibility);
    let submission = build_draft_submission(
        db,
        &order_id,
        &group.scope,
        &supplier_name,
        computed.totals,
        session,
    )
    .await?;
    let submission_id = PurchaseOrderSubmissionId::new(submission.base.id.clone());
    let mut submission_lines = Vec::with_capacity(computed.lines.len());
    for (index, line) in computed.lines.iter().enumerate() {
        submission_lines.push(build_submission_line(&submission_id, (index + 1) as u32, line)?);
    }
    order.attach_draft_submission(submission.base.id.clone().into())?;
    let write = PreparedDraftWrite {
        sales_order,
        order: &order,
        submission: &submission,
        lines: &submission_lines,
        actor: command.actor,
    };
    write_prepared_draft(db, rbac, &write, session).await?;
    let submitted = submit_created_draft_in_session(
        db,
        sales_order,
        &order.base.id,
        command.actor,
        command.req.idempotency_key.as_str(),
        session,
    )
    .await?;
    write_creation_receipt(
        db,
        command,
        &order.base.id,
        submitted.purchase_no,
        submitted.lock_version,
        session,
    )
    .await
}

/// 一组已计算金额的本次采购行。
struct ComputedSelection {
    /// 已舍入行金额汇总。
    totals: (Amount, Amount, Amount),
    /// 逐行成本与金额。
    lines: Vec<ComputedLine>,
}

/// 单条已计算采购行。
struct ComputedLine {
    /// 事务内选择行。
    selected: SelectedLine,
    /// 含税成本。
    cost: UnitPrice,
    /// 含税金额。
    gross: Amount,
    /// 不含税金额。
    net: Amount,
    /// 税额。
    tax: Amount,
}

/// 计算选中行金额与表头汇总。
///
/// # 参数
/// * `selected_lines` - 事务内校验通过的本次采购行
///
/// # 返回
/// 返回逐行已舍入金额及汇总。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 表头只汇总逐行舍入后的金额。
fn compute_selected_lines(
    selected_lines: &[SelectedLine],
    responsibility: FulfillmentResponsibility,
) -> ComputedSelection {
    let mut gross_total = zero_amount();
    let mut net_total = zero_amount();
    let mut tax_total = zero_amount();
    let mut lines = Vec::with_capacity(selected_lines.len());
    for selected in selected_lines {
        let cost = supply_cost(&selected.basis.supply.revision, responsibility);
        let (gross, net, tax) = line_amounts(
            cost,
            selected.quantity,
            selected.basis.supply.revision.input_tax_rate,
        );
        gross_total = gross_total.checked_add(gross);
        net_total = net_total.checked_add(net);
        tax_total = tax_total.checked_add(tax);
        lines.push(ComputedLine {
            selected: selected.clone(),
            cost,
            gross,
            net,
            tax,
        });
    }
    ComputedSelection {
        totals: (gross_total, net_total, tax_total),
        lines,
    }
}

/// 构造采购草稿提交头。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order_id` - 新采购单主键
/// * `scope` - 精确拆分范围
/// * `supplier_name` - 供应商名称快照
/// * `totals` - 表头金额三元组
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回采购草稿提交头。
///
/// # 错误
/// 供应商或商务版本缺失、快照字段非法时返回错误。
///
/// # 关键业务约束
/// 供应商修订和付款条件在创建事务内重新读取并冻结。
async fn build_draft_submission(
    db: &mongodb::Database,
    order_id: &PurchaseOrderId,
    scope: &BasisScope,
    supplier_name: &str,
    totals: (Amount, Amount, Amount),
    executor: &mut dyn Executor,
) -> Result<PurchaseOrderSubmission> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(&scope.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let revision_id = supplier
        .current_commercial_profile_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("供应商缺少商务结算版本".to_string()))?;
    let payment_term = SupplierPaymentTerm::parse(&scope.payment_term_code)?;
    PurchaseOrderSubmission::new(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmissionData {
            purchase_order_id: order_id.clone(),
            submission_no: format!("DRAFT-{}", &next_id()[..8]),
            supplier_id: scope.supplier_id.clone(),
            purchase_type: scope.purchase_type,
            fulfillment_responsibility: scope.fulfillment_responsibility,
            supplier_revision_id: revision_id,
            supplier_snapshot: SupplierSnapshot::new(supplier_name.to_string())?,
            payment_term_snapshot: entities::purchase_order::PaymentTermSnapshot::new(
                payment_term.code().to_string(),
                payment_term.prepay_gate(),
                None,
                None,
            )?,
            gross_amount: totals.0,
            net_amount: totals.1,
            tax_amount: totals.2,
        },
    )
    .map_err(Into::into)
}

/// 构造单条采购草稿行。
///
/// # 参数
/// * `submission_id` - 所属采购提交
/// * `line_no` - 提交内行号
/// * `line` - 已计算本次采购行
///
/// # 返回
/// 返回带稳定销售行和销售当前版本行的采购提交行。
///
/// # 错误
/// 实体不变式失败时返回错误。
///
/// # 关键业务约束
/// `quantity` 与 `allocated_quantity` 均等于本次事务内校验数量。
fn build_submission_line(
    submission_id: &PurchaseOrderSubmissionId,
    line_no: u32,
    line: &ComputedLine,
) -> Result<PurchaseOrderSubmissionLine> {
    let basis = &line.selected.basis;
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(next_id()),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: submission_id.clone(),
            line_no,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(entities::ids::ProcurementConfirmationLineId::new(
                stable_line_id(basis).to_string(),
            )),
            sku_id: Some(basis.coverage.goods_line.sku_id.clone()),
            sku_revision_id: Some(basis.coverage.goods_line.sku_revision_id.clone()),
            product_name_snapshot: Some(basis.coverage.revision_line.item_name_snapshot.clone()),
            specification_snapshot: basis.coverage.revision_line.spec_snapshot.clone(),
            quantity: Some(line.selected.quantity),
            base_unit_code: Some(basis.coverage.goods_line.base_unit_code.clone()),
            unit_cost_gross: Some(line.cost),
            gross_amount: line.gross,
            net_amount: line.net,
            tax_amount: line.tax,
            input_tax_rate: Some(basis.supply.revision.input_tax_rate),
            expected_delivery_date: Some(line.selected.expected_delivery_date),
            sales_order_line_id: Some(basis.coverage.revision_line.sales_order_line_id.clone()),
            sales_order_revision_line_id: Some(entities::ids::SalesOrderRevisionLineId::new(
                basis.coverage.revision_line.base.id.clone(),
            )),
            sales_order_submission_line_id: None,
            allocated_quantity: Some(line.selected.quantity),
        },
    )
    .map_err(Into::into)
}

/// 写入采购草稿聚合、单据注册和审批绑定。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `write` - 来源销售单、采购聚合与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 写入成功返回 `Ok(())`。
///
/// # 错误
/// 审批绑定、单据注册或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 本函数只写入草稿聚合；正式号与审批启动由随后的提交步骤完成。
async fn write_prepared_draft(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    write: &PreparedDraftWrite<'_>,
    session: &mut ClientSession,
) -> Result<()> {
    let organization_id = purchase_order_responsible_org_id(write.sales_order)?;
    let _ = purchase_order_object_readable(&organization_id, write.actor.id())?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::PurchaseOrder,
        business_object_id: write.order.base.id.clone(),
        business_object_version: write.order.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: write.actor.id().to_string(),
        },
    };
    let binding = bind_published_definition_on_document_create(db, rbac, &bind_command, write.actor, session)
        .await?
        .ok_or_else(|| Error::Internal("采购单必须绑定已发布定义".to_string()))?;
    let mut document = new_registered_document(&write.order.base.id, DocumentType::PurchaseOrder, "")?;
    attach_published_binding(&mut document, binding)?;
    db.purchase_orders().create(write.order, session).await?;
    db.business_documents().create(&document, session).await?;
    db.purchase_order_submissions()
        .create(write.submission, session)
        .await?;
    for line in write.lines {
        db.purchase_order_submission_lines().create(line, session).await?;
    }
    sync_procurement_tasks_for_sales_order(db, &write.order.sales_order_id, session).await?;
    Ok(())
}

/// 写入提交后的采购创建命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `command` - 原始请求、收据身份与审计操作人
/// * `purchase_order_id` - 采购单主键
/// * `purchase_no` - 提交后正式号
/// * `lock_version` - 提交后乐观锁版本
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回可回放的创建结果。
///
/// # 错误
/// 收据序列化或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 收据必须与提交后正式号同事务落库，回放不得返回空单号。
async fn write_creation_receipt(
    db: &mongodb::Database,
    command: &CreateBasisCommand<'_>,
    purchase_order_id: &str,
    purchase_no: String,
    lock_version: u64,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrderResult> {
    let receipt = CreationReceipt {
        purchase_order_id: purchase_order_id.to_string(),
        purchase_no,
        lock_version,
    };
    let audit = command.actor.clone().resource_log_with_id(
        command.audit_id.to_string(),
        CREATE_ACTION,
        "purchase_order",
        purchase_order_id.to_string(),
        Some(
            PurchaseCommandReceipt::new(command.request_fingerprint.to_string(), receipt.clone())
                .encode_message()?,
        ),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(receipt.into_result(false))
}

/// 查找客户端选择的精确依据。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `groups` - 当前可用依据集合
/// * `basis_id` - 客户端依据 ID
/// * `work_item_id` - 当前开放供给分配任务
///
/// # 返回
/// 返回与当前 guard、当前版本及精确范围完全匹配的依据。
///
/// # 错误
/// 依据不存在或已失效时返回统一的剩余数量变化冲突。
///
/// # 关键业务约束
/// 不接受旧 guard 或旧销售版本生成的依据 ID。
fn find_requested_group<'a>(
    order: &SalesOrder,
    groups: &'a [BasisGroup],
    basis_id: &str,
    work_item_id: &str,
) -> Result<&'a BasisGroup> {
    groups
        .iter()
        .find(|group| basis_id_for(order, group, work_item_id, None) == basis_id)
        .ok_or_else(procurement_quantity_changed)
}

/// 校验请求表头与依据精确范围一致。
///
/// # 参数
/// * `req` - 创建请求
/// * `scope` - 依据精确范围
///
/// # 返回
/// 一致时返回 `Ok(())`。
///
/// # 错误
/// 采购类型或付款条件不一致时返回校验错误。
///
/// # 关键业务约束
/// 客户端不能把一个依据改造成另一拆分范围。
fn ensure_request_scope(req: &CreatePurchaseOrderFromBasisRequest, scope: &BasisScope) -> Result<()> {
    if req.purchase_type != scope.purchase_type {
        return Err(Error::ValidationError("采购类型与创建依据不一致".to_string()));
    }
    if req.payment_term_code.trim() != scope.payment_term_code {
        return Err(Error::ValidationError("付款条件与创建依据不一致".to_string()));
    }
    Ok(())
}

/// 校验采购单初始责任人可以完成其责任类型对应的后续履约操作。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 与采购创建事务授权版本一致的 RBAC 服务
/// * `responsibility` - 本单履约责任
/// * `owner_user_id` - 创建后冻结为采购单责任人的账号
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 入仓责任或责任人具备完整履约权限时返回成功。
///
/// # 错误
/// 责任人账号不可用、缺少对应完整履约权限，或账号与 RBAC 查询失败时返回错误。
async fn ensure_initial_purchase_order_owner(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    responsibility: FulfillmentResponsibility,
    owner_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(business_object_type) = responsibility.owner_fulfillment_object_type() else {
        return Ok(());
    };
    crate::fulfillment::task::ensure_fulfillment_owner_eligible(
        db,
        rbac,
        owner_user_id,
        business_object_type,
        executor,
    )
    .await
    .map_err(|error| {
        contextualize_fulfillment_owner_error(
            error,
            "当前采购责任人账号不可用或缺少后续履约权限，请先调整角色后再创建采购单",
        )
    })
}

/// 校验并解析采购单目标收货仓。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 与采购创建事务授权版本一致的 RBAC 服务
/// * `responsibility` - 本单履约责任
/// * `requested_id` - 客户端指定的目标仓库
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 仓库履约返回存在且启用的目标仓库，其他履约返回空。
///
/// # 错误
/// 仓库履约未指定目标仓、仓库不存在或停用、入库经办人不可用或权限不足，
/// 或非仓库履约携带目标仓时返回错误。
async fn resolve_target_warehouse(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    responsibility: FulfillmentResponsibility,
    requested_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<WarehouseId>> {
    let normalized = requested_id.map(str::trim).filter(|value| !value.is_empty());
    match responsibility {
        FulfillmentResponsibility::Warehouse => {
            let id = normalized
                .map(|value| WarehouseId::new(value.to_string()))
                .ok_or_else(|| Error::ValidationError("仓库履约必须先选择目标收货仓".to_string()))?;
            let warehouse = db
                .warehouses()
                .find_by_id(&id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("目标仓库不存在，请重新选择".to_string()))?;
            if !warehouse.is_active() {
                return Err(Error::ValidationError(
                    "目标仓库已停用，请重新选择后再创建采购单".to_string(),
                ));
            }
            let handler_user_id = warehouse
                .fulfillment_handler(WarehouseFulfillmentOperation::Receipt)
                .map_err(|_| {
                    Error::ValidationError("目标仓库未配置合格入库经办人，请先完成仓库责任配置".to_string())
                })?;
            crate::fulfillment::task::ensure_fulfillment_owner_eligible(
                db,
                rbac,
                handler_user_id,
                "purchase_receipt",
                executor,
            )
            .await
            .map_err(|error| {
                contextualize_fulfillment_owner_error(
                    error,
                    "目标仓库入库经办人账号不可用或权限不足，请先更新仓库责任配置",
                )
            })?;
            Ok(Some(id))
        }
        _ if normalized.is_some() => Err(Error::ValidationError("非仓库履约不能指定目标收货仓".to_string())),
        _ => Ok(None),
    }
}

/// 把责任资格失败转换为当前创建场景可执行的校验提示，同时保留基础设施错误。
fn contextualize_fulfillment_owner_error(error: Error, message: &str) -> Error {
    match error {
        Error::BusinessLogicError(_) => Error::ValidationError(message.to_string()),
        other => other,
    }
}

/// 按事务内最新依据校验逐行本次数量。
///
/// # 参数
/// * `requested` - 已规范化请求行
/// * `group` - guard 后重算的最新精确依据
///
/// # 返回
/// 返回按请求稳定行排序的已选择采购行。
///
/// # 错误
/// 请求行不属于依据，或数量超过最新剩余量/供应商可供上限时返回冲突。
///
/// # 关键业务约束
/// 同时校验 `quantity <= remaining` 与 `quantity <= min(remaining, available)`。
pub(super) fn validate_requested_quantities(
    requested: &[RequestedLine],
    group: &BasisGroup,
) -> Result<Vec<SelectedLine>> {
    let mut selected = Vec::with_capacity(requested.len());
    for requested_line in requested {
        let basis = group
            .lines
            .iter()
            .find(|line| stable_line_id(line) == requested_line.sales_order_line_id)
            .ok_or_else(procurement_quantity_changed)?;
        if requested_line.quantity > basis.coverage.summary.remaining_quantity
            || requested_line.quantity > basis.max_create_quantity
        {
            return Err(procurement_quantity_changed());
        }
        let sales_due = business_date_of(basis.coverage.goods_line.fulfillment_due_at)?;
        ensure_expected_delivery_within_sales_due(requested_line.expected_delivery_date, sales_due)?;
        selected.push(SelectedLine {
            basis: basis.clone(),
            quantity: requested_line.quantity,
            expected_delivery_date: requested_line.expected_delivery_date,
        });
    }
    Ok(selected)
}

/// 校验采购预计交付日不突破销售对客户的承诺期限。
///
/// # 参数
/// * `expected_delivery_date` - 采购确认的预计交付日
/// * `sales_due` - 销售对客户承诺的最晚交付日
///
/// # 返回
/// 预计交付日不晚于销售承诺期限时返回 `Ok(())`。
///
/// # 错误
/// 预计交付日晚于销售承诺期限时返回校验错误。
fn ensure_expected_delivery_within_sales_due(
    expected_delivery_date: BusinessDate,
    sales_due: BusinessDate,
) -> Result<()> {
    if expected_delivery_date > sales_due {
        return Err(Error::ValidationError(format!(
            "预计交付日不能晚于销售承诺期限 {sales_due}"
        )));
    }
    Ok(())
}

/// 从依据 ID 提取销售单稳定身份。
///
/// # 参数
/// * `basis_id` - `{sales_order_id}:{sha256}` 形式的依据 ID
///
/// # 返回
/// 返回销售单 ID。
///
/// # 错误
/// 依据 ID 形态非法时返回 `NotFound`。
///
/// # 关键业务约束
/// 不兼容旧 `{sales_order_id}:{supplier_id}` 依据 ID。
fn parse_basis_sales_order_id(basis_id: &str) -> Result<SalesOrderId> {
    let (sales_order_id, digest) = basis_id
        .trim()
        .split_once(':')
        .ok_or_else(|| Error::NotFound("采购创建依据不存在".to_string()))?;
    if sales_order_id.is_empty()
        || digest.len() != 64
        || !digest.bytes().all(|value| value.is_ascii_hexdigit())
    {
        return Err(Error::NotFound("采购创建依据不存在".to_string()));
    }
    Ok(SalesOrderId::new(sales_order_id.to_string()))
}

/// 查询并校验采购创建幂等收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `audit_id` - 稳定收据 ID
/// * `expected_fingerprint` - 当前命令载荷指纹
/// * `actor` - 当前操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 收据不存在返回 `None`；存在且一致返回原创建结果并标记回放。
///
/// # 错误
/// 同键异载荷、收据身份不一致、收据损坏或采购单缺失时返回错误。
///
/// # 关键业务约束
/// 事务前、事务内和事务失败后均复用同一校验逻辑。
async fn replay_creation(
    db: &mongodb::Database,
    audit_id: &str,
    expected_fingerprint: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<CreatePurchaseOrderResult>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, executor).await? else {
        return Ok(None);
    };
    let receipt = match PurchaseCommandReceipt::<CreationReceipt>::decode(
        &audit,
        actor.id(),
        CREATE_ACTION,
        None,
        expected_fingerprint,
    ) {
        Ok(receipt) => receipt,
        Err(PurchaseCommandReceiptError::IdentityMismatch | PurchaseCommandReceiptError::PayloadConflict) => {
            return Err(Error::ConflictError("幂等键已用于不同采购创建命令".to_string()));
        }
        Err(PurchaseCommandReceiptError::Corrupted(message)) => {
            return Err(Error::Internal(message));
        }
    };
    if audit.resource_id.as_deref() != Some(receipt.payload().purchase_order_id.as_str()) {
        return Err(Error::ConflictError(
            "采购创建幂等收据与业务资源不一致".to_string(),
        ));
    }
    let order = db
        .purchase_orders()
        .find_by_id(&receipt.payload().purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::Internal("采购创建幂等收据引用的采购单不存在".to_string()))?;
    if order.base.id != receipt.payload().purchase_order_id {
        return Err(Error::ConflictError(
            "采购创建幂等收据与当前采购单不一致".to_string(),
        ));
    }
    Ok(Some(receipt.into_payload().into_result(true)))
}

impl CreationReceipt {
    /// 转换为采购创建响应。
    ///
    /// # 参数
    /// * `replayed` - 是否来自幂等收据回放
    ///
    /// # 返回
    /// 返回 API 创建结果。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 业务引用恒为原采购单 ID。
    fn into_result(self, replayed: bool) -> CreatePurchaseOrderResult {
        CreatePurchaseOrderResult {
            purchase_order_id: self.purchase_order_id.clone(),
            purchase_no: self.purchase_no,
            lock_version: self.lock_version,
            replayed,
            reference: self.purchase_order_id,
        }
    }
}

/// 将精确时间转换为上海业务自然日。
///
/// # 参数
/// * `instant` - 销售履约期限
///
/// # 返回
/// 返回 Asia/Shanghai 自然日。
///
/// # 错误
/// 时区或日期构造失败时返回内部错误。
///
/// # 关键业务约束
/// 不按 UTC 日期截断。
fn business_date_of(instant: Instant) -> Result<BusinessDate> {
    let business_tz = FixedOffset::east_opt(8 * 60 * 60)
        .ok_or_else(|| Error::Internal("无法形成 Asia/Shanghai 时区".to_string()))?;
    let naive = instant.as_utc().with_timezone(&business_tz).date_naive();
    BusinessDate::from_ymd(naive.year(), naive.month(), naive.day())
        .ok_or_else(|| Error::Internal("履约期限日期非法".to_string()))
}

/// 返回统一的采购剩余或供给变化冲突。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回 HTTP 409 对应的稳定业务错误。
///
/// # 错误
/// 无。
pub(super) fn procurement_quantity_changed() -> Error {
    Error::ConflictError("可分配供给数量已更新，请刷新后重试".to_string())
}

/// 返回合法采购数量零值。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回六位精度数量零值。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 只用于边界比较，不代表缺失业务数量。
fn zero_quantity() -> Quantity {
    Quantity::from_str("0").expect("零数量合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::time::Instant;

    use super::{business_date_of, ensure_expected_delivery_within_sales_due, parse_basis_sales_order_id};

    /// 上海零点对应前一日 UTC 时仍还原业务自然日。
    #[test]
    fn business_date_uses_shanghai_timezone() {
        let unix_secs = chrono::DateTime::parse_from_rfc3339("2026-08-23T00:00:00+08:00")
            .expect("测试时间合法")
            .timestamp();

        let date = business_date_of(Instant::from_unix_secs(unix_secs)).expect("业务日期合法");

        assert_eq!(date.to_string(), "2026-08-23");
    }

    /// 新依据 ID 只接受销售单加 SHA-256，不兼容旧供应商拼接形式。
    #[test]
    fn basis_id_parser_rejects_legacy_shape() {
        let digest = entities::purchase_order::digest_parts(["scope".to_string()]);
        assert_eq!(
            parse_basis_sales_order_id(&format!("so-1:{digest}"))
                .unwrap()
                .to_string(),
            "so-1"
        );
        assert!(parse_basis_sales_order_id("so-1:supplier-1").is_err());
    }

    /// 采购预计交付日可以早于或等于销售承诺期限，但不得晚于该期限。
    #[test]
    fn expected_delivery_must_not_exceed_sales_due() {
        let sales_due = entities::common::time::BusinessDate::from_str("2026-09-10").unwrap();
        let earlier = entities::common::time::BusinessDate::from_str("2026-09-09").unwrap();
        let later = entities::common::time::BusinessDate::from_str("2026-09-11").unwrap();

        assert!(ensure_expected_delivery_within_sales_due(earlier, sales_due).is_ok());
        assert!(ensure_expected_delivery_within_sales_due(sales_due, sales_due).is_ok());
        assert!(ensure_expected_delivery_within_sales_due(later, sales_due).is_err());
    }

    /// 验证采购依据创建的操作人授权提交栅栏。
    ///
    /// 服务层必须冻结账号与权限快照，并用同一 policy revision CAS 提交业务事务。
    #[test]
    fn create_from_basis_binds_actor_authorization_to_commit() {
        let production = include_str!("creation_basis.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert!(production.contains("authorize_actor_permission(actor, CREATE_PERMISSION)"));
        assert!(production.contains("ensure_purchase_order_actor_account"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
        assert!(production.contains("ensure_initial_purchase_order_owner"));
        assert!(production.contains("ensure_fulfillment_owner_eligible"));
        assert!(production.contains("submit_created_draft_in_session"));
    }

    /// 创建依据路径必须使用批量事实加载，旧逐行供给与名称查询已删除。
    #[test]
    fn creation_basis_uses_batch_facts_loader() {
        let production = include_str!("creation_basis.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert!(
            production.contains("load_creation_basis_facts"),
            "必须使用批量事实加载"
        );
        assert!(
            production.contains("basis_groups_and_facts"),
            "事务内必须复用同一批事实"
        );
        assert!(
            !production.contains("list_active_offerings_by_sku("),
            "逐 SKU 供给查询已删除"
        );
        assert!(
            !production.contains("cached_settlement_terms"),
            "逐供应商付款条件缓存已删除"
        );
        assert!(
            !production.contains("resolve_supplier_name"),
            "逐供应商名称查询已删除"
        );
        assert!(
            !production.contains("fn normalize_requested_lines"),
            "请求行规范化已下沉实体"
        );
        assert!(
            !production.contains("fn basis_id_for") && !production.contains("fn basis_scope_key"),
            "依据身份规则已下沉实体"
        );
    }
}
