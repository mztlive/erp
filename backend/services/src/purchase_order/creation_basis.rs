//! 采购创建依据与按销售当前版本剩余数量建单。
//!
//! 创建依据由销售单当前版本的 `GOODS_SERVICE` 行、当前采购覆盖数量和供应商
//! 当前合格供给共同形成。依据精确到销售当前版本、供应商、采购类型、付款条件与
//! 履约责任；一次依据命令只创建一张采购单。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use chrono::{Datelike, FixedOffset};
use database::{
    AccessControlExt, DocumentRegistryExt, Executor, NoTransaction, PartyExt, PurchaseOrderExt,
    SalesOrderExt, SupplierExt, SupplierOfferingExt, WorkItemExt,
};
use entities::common::time::{BusinessDate, Instant};
use entities::document_registry::DocumentType;
use entities::ids::{
    PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId,
    SupplierAccountId,
};
use entities::money::{line_amounts, Amount, Quantity, UnitPrice};
use entities::purchase_order::{
    FulfillmentResponsibility, PurchaseLineType, PurchaseOrder, PurchaseOrderData, PurchaseOrderSubmission,
    PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, PurchaseType,
    SupplierSnapshot,
};
use entities::sales_order::types::FulfillmentMode;
use entities::sales_order::{CommercialStatus, SalesOrder, SalesOrderRevision};
use entities::supplier_offering::{
    AvailabilityStatus, SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision,
};
use id_generator::next_id;
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use validator::Validate;

use super::adapter::{purchase_order_object_readable, purchase_order_responsible_org_id};
use super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::coverage::{load_sales_procurement_coverage, SalesProcurementCoverageLine};
use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreationBasisLineView,
    CreationBasisListParams, CreationBasisView,
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

const CREATE_ACTION: &str = "purchase_order.create_from_basis";
const CREATE_PERMISSION: &str = "purchase_order:create";
const CREATE_RECEIPT_PREFIX: &str = "purchase-order-create-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

/// 单条销售当前版本明细的合格供应商供给。
#[derive(Debug, Clone)]
struct LineSupply {
    /// 供给稳定身份。
    offering: SupplierOffering,
    /// 当前有效商业条款修订。
    revision: SupplierOfferingRevision,
    /// 当前可供投影。
    availability: SupplierOfferingAvailability,
}

/// 一条可进入精确创建依据的销售当前版本行。
#[derive(Debug, Clone)]
struct BasisLine {
    /// 当前销售版本行及采购覆盖摘要。
    coverage: SalesProcurementCoverageLine,
    /// 本供应商被确定选用的供给。
    supply: LineSupply,
    /// 本供应商本次最多可创建数量。
    max_create_quantity: Quantity,
}

/// 一张采购单的精确拆分维度。
#[derive(Debug, Clone, PartialEq, Eq)]
struct BasisScope {
    /// 唯一供应商。
    supplier_id: SupplierAccountId,
    /// 采购类型。
    purchase_type: PurchaseType,
    /// 付款条件。
    payment_term_code: String,
    /// 履约责任。
    fulfillment_responsibility: FulfillmentResponsibility,
}

/// 一条精确采购创建依据。
#[derive(Debug, Clone)]
struct BasisGroup {
    /// 销售当前版本。
    revision: SalesOrderRevision,
    /// 精确拆分维度。
    scope: BasisScope,
    /// 可采购明细。
    lines: Vec<BasisLine>,
}

/// 已规范化的本次采购行请求。
#[derive(Debug, Clone)]
struct RequestedLine {
    /// 稳定销售行。
    sales_order_line_id: String,
    /// 本次采购数量。
    quantity: Quantity,
}

/// 已通过事务内最新剩余量校验的采购行。
#[derive(Debug, Clone)]
struct SelectedLine {
    /// 当前依据行。
    basis: BasisLine,
    /// 本次采购数量。
    quantity: Quantity,
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
struct CreateBasisCommand<'a> {
    /// 来源销售单。
    sales_order_id: &'a SalesOrderId,
    /// 原始创建请求。
    req: &'a CreatePurchaseOrderFromBasisRequest,
    /// 已规范化逐行数量。
    requested_lines: &'a [RequestedLine],
    /// 稳定命令收据 ID。
    audit_id: &'a str,
    /// 命令载荷指纹。
    request_fingerprint: &'a str,
    /// 审计操作人。
    actor: &'a AuditActor,
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
    /// 同事务命令收据审计。
    audit: &'a entities::AuditLog,
    /// 审计操作人。
    actor: &'a AuditActor,
}

impl PurchaseOrderService {
    /// 查询当前账号开放采购任务范围内仍有剩余量的精确采购创建依据。
    ///
    /// # 参数
    /// * `params` - 可选销售单与采购建单任务筛选
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
        let mut views = Vec::new();
        for task in tasks {
            if task.responsibility_key().is_none() || task.responsibility_scope_ids().is_empty() {
                return Err(Error::ConflictError("采购建单任务缺少冻结责任范围".to_string()));
            }
            let Some(order) = orders.get(&task.business_object_id) else {
                continue;
            };
            let groups = basis_groups_for_order(
                &self.db,
                order,
                task.responsibility_scope_ids(),
                &mut NoTransaction,
            )
            .await?;
            let owner_name = owner_names.get(&order.stable.created_by).cloned();
            for group in groups {
                views.push(
                    build_basis_view(&self.db, order, &group, owner_name.clone(), &task.base.id).await?,
                );
            }
        }
        Ok(views)
    }

    /// 依据精确拆分维度和逐行本次数量创建一张采购草稿。
    ///
    /// # 参数
    /// * `req` - 精确依据、逐行数量与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建采购单；同一幂等键与同一载荷重复提交时返回原结果。
    ///
    /// # 错误
    /// 操作账号不可登录或缺少采购创建权限、依据失效、数量非正或超过事务内最新
    /// 剩余/可供量、幂等键载荷冲突、并发冲突、审批绑定或仓储写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 操作人授权版本通过 policy CAS 与提交绑定；事务内再以销售单 CAS guard 串行化并重算剩余量。
    pub async fn create_from_basis(
        &self,
        req: CreatePurchaseOrderFromBasisRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        req.validate()?;
        let requested_lines = normalize_requested_lines(&req)?;
        let request_fingerprint = create_request_fingerprint(&req, &requested_lines);
        let audit_id = create_command_audit_id(actor.id(), &req.idempotency_key);
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

/// 在 MongoDB 事务内串行化、重算并写入一张采购草稿。
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
/// guard CAS 成功后必须再次按采购当前指针计算剩余量。
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
    let latest_groups = basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    let latest = latest_groups
        .into_iter()
        .find(|group| group.scope == selected.scope)
        .ok_or_else(procurement_quantity_changed)?;
    let selected_lines = validate_requested_quantities(command.requested_lines, &latest)?;
    persist_basis_draft(db, rbac, &order, &latest, &selected_lines, command, session).await
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
async fn load_effective_sales_order(
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

/// 由销售当前版本、当前覆盖和当前供给形成精确依据集合。
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
/// 仅对任务冻结范围内的稳定销售行查询供应商供给；同一依据内供应商、采购类型、付款条件和履约责任完全一致。
async fn basis_groups_for_order(
    db: &mongodb::Database,
    order: &SalesOrder,
    responsibility_scope_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<BasisGroup>> {
    if order.commercial_status != CommercialStatus::Effective {
        return Ok(Vec::new());
    }
    let responsibility_scope_ids = responsibility_scope_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let coverage = load_sales_procurement_coverage(db, order, executor).await?;
    let mut payment_terms = HashMap::new();
    let mut groups: Vec<BasisGroup> = Vec::new();
    for line in coverage.lines {
        if !responsibility_scope_ids.contains(line.revision_line.sales_order_line_id.as_ref())
            || line.summary.remaining_quantity <= zero_quantity()
        {
            continue;
        }
        let supplies = qualified_supplies_for_line(db, &line, executor).await?;
        append_line_supplies(
            db,
            &coverage.revision,
            line,
            supplies,
            &mut payment_terms,
            &mut groups,
            executor,
        )
        .await?;
    }
    for group in &mut groups {
        group
            .lines
            .sort_by(|left, right| stable_line_id(left).cmp(stable_line_id(right)));
    }
    groups.sort_by_key(|group| basis_scope_key(&group.scope));
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
/// * `db` - MongoDB 数据库
/// * `revision` - 销售当前版本
/// * `line` - 当前销售版本目标行
/// * `supplies` - 每供应商一条确定供给
/// * `payment_terms` - 供应商付款条件缓存
/// * `groups` - 待追加依据集合
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 追加完成返回 `Ok(())`。
///
/// # 错误
/// 付款条件查询或数量计算失败时返回错误。
///
/// # 关键业务约束
/// 有限可供量使用 `min(remaining, available)`，不因不足全量而丢弃供应商。
async fn append_line_supplies(
    db: &mongodb::Database,
    revision: &SalesOrderRevision,
    line: SalesProcurementCoverageLine,
    supplies: Vec<LineSupply>,
    payment_terms: &mut HashMap<String, String>,
    groups: &mut Vec<BasisGroup>,
    executor: &mut dyn Executor,
) -> Result<()> {
    for supply in supplies {
        let supplier_id = supply.offering.supplier_id.clone();
        let supplier_key = supplier_id.to_string();
        let payment_term_code = match payment_terms.get(&supplier_key) {
            Some(value) => value.clone(),
            None => {
                let value = resolve_payment_term_code(db, &supplier_id, executor).await?;
                payment_terms.insert(supplier_key, value.clone());
                value
            }
        };
        let mode = line.goods_line.fulfillment_mode;
        let scope = BasisScope {
            supplier_id,
            purchase_type: purchase_type_from_mode(mode),
            payment_term_code,
            fulfillment_responsibility: fulfillment_from_mode(mode),
        };
        let max_create_quantity = maximum_create_quantity(
            line.summary.remaining_quantity,
            supply.availability.available_quantity,
        )?;
        if max_create_quantity <= zero_quantity() {
            continue;
        }
        let basis_line = BasisLine {
            coverage: line.clone(),
            supply,
            max_create_quantity,
        };
        if let Some(group) = groups.iter_mut().find(|group| group.scope == scope) {
            group.lines.push(basis_line);
        } else {
            groups.push(BasisGroup {
                revision: revision.clone(),
                scope,
                lines: vec![basis_line],
            });
        }
    }
    Ok(())
}

/// 查询一条销售当前版本行的合格供给，并为每个供应商确定一条稳定供给。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `line` - 销售当前版本目标行
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回按供应商和供给 ID 稳定排序、每供应商最多一条的供给。
///
/// # 错误
/// 供给修订或可供投影查询失败时返回错误。
///
/// # 关键业务约束
/// 仅 ACTIVE、条款当前有效且 availability 为 AVAILABLE 的供给合格。
async fn qualified_supplies_for_line(
    db: &mongodb::Database,
    line: &SalesProcurementCoverageLine,
    executor: &mut dyn Executor,
) -> Result<Vec<LineSupply>> {
    let offerings = db
        .purchase_order()
        .list_active_offerings_by_sku(&line.goods_line.sku_id, executor)
        .await?;
    let mut seen_suppliers = HashSet::new();
    let mut supplies = Vec::new();
    for offering in offerings {
        if seen_suppliers.contains(&offering.supplier_id.to_string()) {
            continue;
        }
        let Some(supply) = qualified_supply(db, offering, executor).await? else {
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
/// * `db` - MongoDB 数据库
/// * `offering` - ACTIVE 供给稳定身份
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 当前合格时返回供给；缺少当前修订、条款失效或不可供时返回 `None`。
///
/// # 错误
/// 仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 可供数量为空表示供应商未给出上限，不等于不可供。
async fn qualified_supply(
    db: &mongodb::Database,
    offering: SupplierOffering,
    executor: &mut dyn Executor,
) -> Result<Option<LineSupply>> {
    let Some(revision_id) = offering.stable.current_revision_id.clone() else {
        return Ok(None);
    };
    let Some(revision) = db
        .supplier_offering_revisions()
        .find_by_id(&revision_id, executor)
        .await?
    else {
        return Ok(None);
    };
    let today = BusinessDate::today();
    if revision.valid_from > today || revision.valid_to.is_some_and(|valid_to| valid_to < today) {
        return Ok(None);
    }
    let Some(availability) = db
        .supplier_offering_availabilities()
        .find_by_offering_id(&offering.base.id.clone().into(), executor)
        .await?
    else {
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
        offering,
        revision,
        availability,
    }))
}

/// 构造一条精确创建依据视图。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 销售稳定单
/// * `group` - 精确依据分组
/// * `sales_owner_name` - 销售负责人展示名
/// * `work_item_id` - 冻结本依据责任范围的开放采购建单任务
///
/// # 返回
/// 返回前端可直接选择逐行数量的依据视图。
///
/// # 错误
/// 供应商名称或日期转换失败时返回错误。
///
/// # 关键业务约束
/// 预计金额按 `max_create_quantity` 逐行舍入后汇总。
async fn build_basis_view(
    db: &mongodb::Database,
    order: &SalesOrder,
    group: &BasisGroup,
    sales_owner_name: Option<String>,
    work_item_id: &str,
) -> Result<CreationBasisView> {
    let supplier_name = resolve_supplier_name(db, &group.scope.supplier_id, &mut NoTransaction)
        .await?
        .unwrap_or_else(|| group.scope.supplier_id.to_string());
    let mut estimated = zero_amount();
    let mut lines = Vec::with_capacity(group.lines.len());
    for line in &group.lines {
        let cost = supply_cost(&line.supply.revision, line.coverage.goods_line.fulfillment_mode);
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
        basis_id: basis_id_for(order, group, work_item_id),
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
        purchase_type: group.scope.purchase_type.as_str().to_string(),
        fulfillment_responsibility: group.scope.fulfillment_responsibility.as_str().to_string(),
        payment_term_code: group.scope.payment_term_code.clone(),
        lines,
        estimated_gross: estimated.to_string(),
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
        expected_delivery_date: business_date_of(line.coverage.goods_line.fulfillment_due_at)?.to_string(),
        product_name: Some(line.coverage.revision_line.item_name_snapshot.clone()),
        specification: line.coverage.revision_line.spec_snapshot.clone(),
        unit: line.coverage.revision_line.unit_snapshot.clone(),
        gross_amount: gross.to_string(),
    })
}

/// 在事务内持久化一张精确依据采购草稿。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `sales_order` - 已完成 guard CAS 的销售单
/// * `group` - guard 后重算得到的最新依据范围
/// * `selected_lines` - 事务内校验通过的本次采购行
/// * `command` - 原始请求、命令收据与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回创建结果。
///
/// # 错误
/// 实体构造、审批绑定或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// `creation_basis_id` 唯一，且本函数只创建一个采购聚合。
async fn persist_basis_draft(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    sales_order: &SalesOrder,
    group: &BasisGroup,
    selected_lines: &[SelectedLine],
    command: &CreateBasisCommand<'_>,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrderResult> {
    let order_id = PurchaseOrderId::new(next_id());
    let mut order = PurchaseOrder::new(
        order_id.clone(),
        PurchaseOrderData {
            purchase_no: String::new(),
            sales_order_id: SalesOrderId::new(sales_order.base.id.clone()),
            sales_order_revision_id: group.revision.base.id.clone().into(),
            creation_basis_id: command.req.basis_id.clone(),
            supplier_id: group.scope.supplier_id.clone(),
            purchase_type: group.scope.purchase_type,
            payment_term_code: group.scope.payment_term_code.clone(),
            fulfillment_responsibility: group.scope.fulfillment_responsibility,
        },
        command.actor.id(),
    )?;
    let supplier_name = resolve_supplier_name(db, &group.scope.supplier_id, session)
        .await?
        .unwrap_or_else(|| group.scope.supplier_id.to_string());
    let computed = compute_selected_lines(selected_lines);
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
    let receipt = CreationReceipt {
        purchase_order_id: order.base.id.clone(),
        purchase_no: order.purchase_no.clone(),
        lock_version: order.base.version,
    };
    let audit = command.actor.clone().resource_log_with_id(
        command.audit_id.to_string(),
        CREATE_ACTION,
        "purchase_order",
        order.base.id.clone(),
        Some(creation_receipt_message(command.request_fingerprint, &receipt)?),
    )?;
    let write = PreparedDraftWrite {
        sales_order,
        order: &order,
        submission: &submission,
        lines: &submission_lines,
        audit: &audit,
        actor: command.actor,
    };
    write_prepared_draft(db, rbac, &write, session).await?;
    Ok(receipt.into_result(false))
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
fn compute_selected_lines(selected_lines: &[SelectedLine]) -> ComputedSelection {
    let mut gross_total = zero_amount();
    let mut net_total = zero_amount();
    let mut tax_total = zero_amount();
    let mut lines = Vec::with_capacity(selected_lines.len());
    for selected in selected_lines {
        let mode = selected.basis.coverage.goods_line.fulfillment_mode;
        let cost = supply_cost(&selected.basis.supply.revision, mode);
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
    let prepay_gate = scope
        .payment_term_code
        .trim()
        .to_uppercase()
        .starts_with("PREPAY");
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
                scope.payment_term_code.clone(),
                prepay_gate,
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
/// 履约期限日期非法或实体不变式失败时返回错误。
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
            expected_delivery_date: Some(business_date_of(basis.coverage.goods_line.fulfillment_due_at)?),
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

/// 写入采购草稿聚合、单据注册、审批绑定和命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `write` - 来源销售单、采购聚合、命令收据与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 写入成功返回 `Ok(())`。
///
/// # 错误
/// 审批绑定、单据注册或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 命令收据与采购聚合必须同事务提交。
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
    db.audit_logs().create(write.audit, session).await?;
    Ok(())
}

/// 查找客户端选择的精确依据。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `groups` - 当前可用依据集合
/// * `basis_id` - 客户端依据 ID
/// * `work_item_id` - 当前开放采购建单任务
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
        .find(|group| basis_id_for(order, group, work_item_id) == basis_id)
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

/// 规范化并校验逐行本次采购数量。
///
/// # 参数
/// * `req` - 创建请求
///
/// # 返回
/// 返回去除首尾空白、数量已类型化且稳定行不重复的请求行。
///
/// # 错误
/// 稳定行重复、数量非法或数量不大于零时返回校验错误。
///
/// # 关键业务约束
/// 同一稳定销售行在一次命令中只能出现一次。
fn normalize_requested_lines(req: &CreatePurchaseOrderFromBasisRequest) -> Result<Vec<RequestedLine>> {
    let mut seen = HashSet::new();
    let mut lines = Vec::with_capacity(req.lines.len());
    for line in &req.lines {
        let sales_order_line_id = line.sales_order_line_id.trim().to_string();
        if !seen.insert(sales_order_line_id.clone()) {
            return Err(Error::ValidationError("本次采购明细包含重复销售行".to_string()));
        }
        let quantity = Quantity::from_str(line.quantity.trim())
            .map_err(|error| Error::ValidationError(format!("本次数量非法: {error}")))?;
        if quantity <= zero_quantity() {
            return Err(Error::ValidationError("本次数量必须大于 0".to_string()));
        }
        lines.push(RequestedLine {
            sales_order_line_id,
            quantity,
        });
    }
    lines.sort_by(|left, right| left.sales_order_line_id.cmp(&right.sales_order_line_id));
    Ok(lines)
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
fn validate_requested_quantities(
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
        selected.push(SelectedLine {
            basis: basis.clone(),
            quantity: requested_line.quantity,
        });
    }
    Ok(selected)
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

/// 形成包含 guard、当前版本、精确范围和逐行供给事实的依据 ID。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `group` - 精确依据分组
/// * `work_item_id` - 冻结本依据责任范围的开放任务
///
/// # 返回
/// 返回 `{sales_order_id}:{sha256}` 稳定依据 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// guard 每次成功创建后推进，使作废释放的剩余量可形成新依据。
fn basis_id_for(order: &SalesOrder, group: &BasisGroup, work_item_id: &str) -> String {
    let mut parts = vec![
        order.base.id.clone(),
        work_item_id.to_string(),
        order.procurement_guard_version.to_string(),
        group.revision.base.id.clone(),
        basis_scope_key(&group.scope),
    ];
    parts.extend(group.lines.iter().map(basis_line_fingerprint));
    format!("{}:{}", order.base.id, digest_parts(&parts))
}

/// 形成单条依据行的供给与剩余量指纹。
///
/// # 参数
/// * `line` - 精确依据行
///
/// # 返回
/// 返回稳定行、当前版本行、数量与供给版本组成的规范化字符串。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 可供投影版本变化会使旧依据失效。
fn basis_line_fingerprint(line: &BasisLine) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        stable_line_id(line),
        line.coverage.revision_line.base.id,
        line.coverage.summary.remaining_quantity,
        line.max_create_quantity,
        line.supply.offering.base.id,
        line.supply.revision.base.id,
        line.supply.availability.base.id,
        line.supply.availability.base.version,
        line.supply
            .availability
            .source_revision_token
            .as_deref()
            .unwrap_or("-"),
    )
}

/// 形成精确拆分范围规范化键。
///
/// # 参数
/// * `scope` - 精确拆分范围
///
/// # 返回
/// 返回供应商、采购类型、付款条件和履约责任拼接键。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 该键只用于分组和指纹，不作为数据库自然键。
fn basis_scope_key(scope: &BasisScope) -> String {
    format!(
        "{}|{}|{}|{}",
        scope.supplier_id,
        scope.purchase_type.as_str(),
        scope.payment_term_code,
        scope.fulfillment_responsibility.as_str(),
    )
}

/// 计算供应商本次最大可创建数量。
///
/// # 参数
/// * `remaining` - 销售当前版本剩余量
/// * `available` - 供应商当前可供上限；空表示无限制
///
/// # 返回
/// 返回 `min(remaining, available)` 或无上限时的 `remaining`。
///
/// # 错误
/// 可供数量为负时返回一致性错误。
///
/// # 关键业务约束
/// 供应不足允许形成部分数量依据。
fn maximum_create_quantity(remaining: Quantity, available: Option<Quantity>) -> Result<Quantity> {
    let Some(available) = available else {
        return Ok(remaining);
    };
    if available < zero_quantity() {
        return Err(Error::BusinessLogicError("供应商可供数量不能为负".to_string()));
    }
    Ok(remaining.min(available))
}

/// 返回依据行的稳定销售行 ID。
///
/// # 参数
/// * `line` - 精确依据行
///
/// # 返回
/// 返回跨销售版本稳定的销售行 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 所有覆盖与请求匹配均使用稳定销售行，不按 SKU 猜测。
fn stable_line_id(line: &BasisLine) -> &str {
    line.coverage.revision_line.sales_order_line_id.as_ref()
}

/// 构造创建命令载荷指纹。
///
/// # 参数
/// * `req` - 创建请求
/// * `lines` - 已规范化并排序的请求行
///
/// # 返回
/// 返回不包含原始幂等键的 SHA-256 指纹。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 同一幂等键用于不同依据、范围或数量时必须冲突。
fn create_request_fingerprint(req: &CreatePurchaseOrderFromBasisRequest, lines: &[RequestedLine]) -> String {
    let mut parts = vec![
        req.work_item_id.trim().to_string(),
        req.basis_id.trim().to_string(),
        req.purchase_type.as_str().to_string(),
        req.payment_term_code.trim().to_string(),
    ];
    parts.extend(
        lines
            .iter()
            .map(|line| format!("{}|{}", line.sales_order_line_id, line.quantity)),
    );
    digest_parts(&parts)
}

/// 生成不暴露原始幂等键的稳定命令收据 ID。
///
/// # 参数
/// * `actor_id` - 操作人 ID
/// * `idempotency_key` - 客户端幂等键
///
/// # 返回
/// 返回审计日志唯一 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 同一操作人同一幂等键只能对应一个创建命令载荷。
fn create_command_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    format!(
        "{CREATE_RECEIPT_PREFIX}{}",
        digest_parts(&[
            actor_id.to_string(),
            CREATE_ACTION.to_string(),
            idempotency_key.to_string(),
        ])
    )
}

/// 编码采购创建命令收据。
///
/// # 参数
/// * `fingerprint` - 命令载荷指纹
/// * `receipt` - 创建结果载荷
///
/// # 返回
/// 返回可写入审计消息的收据文本。
///
/// # 错误
/// JSON 序列化失败时返回内部错误。
///
/// # 关键业务约束
/// 收据不得包含原始幂等键。
fn creation_receipt_message(fingerprint: &str, receipt: &CreationReceipt) -> Result<String> {
    let result = serde_json::to_string(receipt)
        .map_err(|error| Error::Internal(format!("采购创建幂等收据序列化失败: {error}")))?;
    Ok(format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={result}"
    ))
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
    if !audit.success
        || audit.actor_id != actor.id()
        || audit.action != CREATE_ACTION
        || audit.resource_type != "purchase_order"
    {
        return Err(Error::ConflictError("幂等键已用于不同采购创建命令".to_string()));
    }
    let receipt = parse_creation_receipt(audit.message.as_deref().unwrap_or_default(), expected_fingerprint)?;
    if audit.resource_id.as_deref() != Some(receipt.purchase_order_id.as_str()) {
        return Err(Error::ConflictError(
            "采购创建幂等收据与业务资源不一致".to_string(),
        ));
    }
    let order = db
        .purchase_orders()
        .find_by_id(&receipt.purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::Internal("采购创建幂等收据引用的采购单不存在".to_string()))?;
    if order.base.id != receipt.purchase_order_id {
        return Err(Error::ConflictError(
            "采购创建幂等收据与当前采购单不一致".to_string(),
        ));
    }
    Ok(Some(receipt.into_result(true)))
}

/// 解析并校验采购创建命令收据。
///
/// # 参数
/// * `message` - 审计消息
/// * `expected_fingerprint` - 当前命令载荷指纹
///
/// # 返回
/// 返回已反序列化创建结果。
///
/// # 错误
/// 收据格式非法、同键异载荷或结果 JSON 损坏时返回错误。
///
/// # 关键业务约束
/// 指纹必须在读取结果前比较。
fn parse_creation_receipt(message: &str, expected_fingerprint: &str) -> Result<CreationReceipt> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("采购创建幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError("幂等键已用于不同采购创建命令".to_string()));
    }
    serde_json::from_str(result)
        .map_err(|error| Error::Internal(format!("采购创建幂等收据结果非法: {error}")))
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

/// 对规范化字符串序列计算 SHA-256。
///
/// # 参数
/// * `parts` - 已按业务稳定顺序排列的字符串片段
///
/// # 返回
/// 返回 64 位小写十六进制摘要。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 每段带长度前缀，避免简单拼接歧义。
fn digest_parts(parts: &[String]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.len().to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// 读取供应商主体当前法定名称。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `supplier_id` - 供应商身份
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回当前主体修订法定名称；关联缺失时返回 `None`。
///
/// # 错误
/// 仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 名称只用于展示和创建快照，不参与供应商稳定关联。
async fn resolve_supplier_name(
    db: &mongodb::Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(supplier) = db.supplier_accounts().find_by_id(supplier_id, executor).await? else {
        return Ok(None);
    };
    let Some(party) = db.parties().find_by_id(&supplier.party_id, executor).await? else {
        return Ok(None);
    };
    let Some(revision_id) = party.stable.current_revision_id.clone() else {
        return Ok(None);
    };
    let revision = db.party_revisions().find_by_id(&revision_id, executor).await?;
    Ok(revision.map(|revision| revision.legal_name))
}

/// 读取供应商当前商务结算版本付款条件。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `supplier_id` - 供应商身份
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回当前付款条件；供应商或商务版本缺失时返回 `NET-30`。
///
/// # 错误
/// 仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 付款条件是精确拆单维度的一部分。
async fn resolve_payment_term_code(
    db: &mongodb::Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<String> {
    let Some(supplier) = db.supplier_accounts().find_by_id(supplier_id, executor).await? else {
        return Ok("NET-30".to_string());
    };
    let Some(revision_id) = supplier.current_commercial_profile_revision_id.clone() else {
        return Ok("NET-30".to_string());
    };
    let revision = db
        .supplier_commercial_profile_revisions()
        .find_by_id(&revision_id, executor)
        .await?;
    Ok(revision
        .map(|revision| revision.payment_term_snapshot)
        .unwrap_or_else(|| "NET-30".to_string()))
}

/// 取供给含税成本。
///
/// # 参数
/// * `revision` - 当前有效供给条款
/// * `mode` - 销售当前版本履约方式
///
/// # 返回
/// 入仓返回集采价，其他方式返回一件代发价。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 成本只从依据确定的当前供给修订读取。
fn supply_cost(revision: &SupplierOfferingRevision, mode: FulfillmentMode) -> UnitPrice {
    match mode {
        FulfillmentMode::CompanyWarehouse => revision.bulk_supply_price_gross,
        FulfillmentMode::SupplierDirect
        | FulfillmentMode::ElectronicDelivery
        | FulfillmentMode::OfflineService => revision.dropship_supply_price_gross,
    }
}

/// 由销售履约方式推导采购类型。
///
/// # 参数
/// * `mode` - 销售履约方式
///
/// # 返回
/// 返回采购类型。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 同一依据只包含相同采购类型。
fn purchase_type_from_mode(mode: FulfillmentMode) -> PurchaseType {
    match mode {
        FulfillmentMode::CompanyWarehouse | FulfillmentMode::SupplierDirect => PurchaseType::Physical,
        FulfillmentMode::ElectronicDelivery => PurchaseType::Virtual,
        FulfillmentMode::OfflineService => PurchaseType::Service,
    }
}

/// 由销售履约方式推导采购履约责任。
///
/// # 参数
/// * `mode` - 销售履约方式
///
/// # 返回
/// 返回采购履约责任。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 履约责任是精确拆单维度，不在一条依据内暗中再拆单。
fn fulfillment_from_mode(mode: FulfillmentMode) -> FulfillmentResponsibility {
    match mode {
        FulfillmentMode::CompanyWarehouse => FulfillmentResponsibility::Warehouse,
        FulfillmentMode::SupplierDirect => FulfillmentResponsibility::SupplierDirect,
        FulfillmentMode::ElectronicDelivery => FulfillmentResponsibility::Electronic,
        FulfillmentMode::OfflineService => FulfillmentResponsibility::Service,
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
fn procurement_quantity_changed() -> Error {
    Error::ConflictError("可采购数量已更新，请刷新后重试".to_string())
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
    use entities::money::Quantity;
    use entities::purchase_order::{FulfillmentResponsibility, PurchaseType};

    use super::{
        business_date_of, digest_parts, maximum_create_quantity, parse_basis_sales_order_id, BasisScope,
    };

    /// 上海零点对应前一日 UTC 时仍还原业务自然日。
    #[test]
    fn business_date_uses_shanghai_timezone() {
        let unix_secs = chrono::DateTime::parse_from_rfc3339("2026-08-23T00:00:00+08:00")
            .expect("测试时间合法")
            .timestamp();

        let date = business_date_of(Instant::from_unix_secs(unix_secs)).expect("业务日期合法");

        assert_eq!(date.to_string(), "2026-08-23");
    }

    /// 有限可供量不足销售剩余时允许形成部分数量。
    #[test]
    fn limited_availability_uses_partial_quantity() {
        let remaining = Quantity::from_str("10").unwrap();
        let available = Quantity::from_str("3.5").unwrap();

        assert_eq!(
            maximum_create_quantity(remaining, Some(available)).unwrap(),
            available
        );
        assert_eq!(maximum_create_quantity(remaining, None).unwrap(), remaining);
    }

    /// 新依据 ID 只接受销售单加 SHA-256，不兼容旧供应商拼接形式。
    #[test]
    fn basis_id_parser_rejects_legacy_shape() {
        let digest = digest_parts(&["scope".to_string()]);
        assert_eq!(
            parse_basis_sales_order_id(&format!("so-1:{digest}"))
                .unwrap()
                .to_string(),
            "so-1"
        );
        assert!(parse_basis_sales_order_id("so-1:supplier-1").is_err());
    }

    /// 精确范围同时区分供应商、类型、付款条件和履约责任。
    #[test]
    fn basis_scope_key_contains_exact_split_dimensions() {
        let scope = BasisScope {
            supplier_id: entities::ids::SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
        };

        assert_eq!(super::basis_scope_key(&scope), "sup-1|PHYSICAL|NET-30|WAREHOUSE");
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
    }
}
