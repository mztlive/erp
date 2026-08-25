//! 按选源结果一次创建多张采购单并提交审批。
//!
//! 销售明细选定供应商后，按供应商、采购类型、付款条件和履约责任拆成多张采购单，
//! 并在同一事务内推进一次采购 guard 后写入全部采购单并启动审批。

use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

use database::{AccessControlExt, Executor, NoTransaction, SalesOrderExt};
use entities::ids::{SalesOrderId, SupplierAccountId};
use entities::money::Quantity;
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::command_receipt::{
    command_receipt_id, command_receipt_message, command_request_fingerprint, parse_command_receipt,
};
use super::creation_basis::{
    basis_groups_for_order, basis_id_for, basis_scope_key, load_effective_sales_order, persist_basis_draft,
    procurement_quantity_changed, stable_line_id, validate_requested_quantities, BasisGroup,
    CreateBasisCommand, RequestedLine,
};
use super::dto::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest, CreatePurchaseOrderResult,
    CreatePurchaseOrdersFromSourcingRequest, CreatePurchaseOrdersFromSourcingResult,
};
use super::procurement_task_sync::load_owned_open_procurement_task;
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
}

/// 已归入一张采购单的选源计划。
#[derive(Debug, Clone)]
struct SourcingDraftPlan {
    /// 命中的精确依据分组。
    group: BasisGroup,
    /// 本单规范化后的逐行数量。
    requested_lines: Vec<RequestedLine>,
}

impl PurchaseOrderService {
    /// 按选源行一次创建多张采购单并提交审批。
    ///
    /// # 参数
    /// * `req` - 来源销售单、建单任务、逐行供应商与数量、幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回按精确拆分维度创建并已提交审批的全部采购单；同一幂等键与同一载荷重复提交时返回原结果。
    ///
    /// # 错误
    /// 操作账号不可登录或缺少采购创建权限、选源行重复或无合格供给、数量非正或超过
    /// 事务内最新剩余/可供量、幂等键载荷冲突、并发冲突、审批绑定、启动审批或仓储写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 同一销售行只能指定一家供应商；供应商、采购类型、付款条件或履约责任任一不同即拆单；
    /// 操作人授权版本通过 policy CAS 与提交绑定，事务内只推进一次销售单采购 guard；
    /// 创建成功即进入审批中，不得留下可编辑草稿。
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

/// 在 MongoDB 事务内按选源计划写入多张采购单并提交审批。
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
/// guard CAS 成功后必须再次按采购当前指针计算剩余量，且本函数只推进一次 guard。
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
    let plans = plan_sourcing_drafts(&groups, assignments)?;
    order.advance_procurement_guard(actor.id())?;
    db.sales_orders().update(&mut order, session).await?;
    let latest_groups = basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
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
    write_sourcing_receipt(
        db,
        audit_id,
        request_fingerprint,
        sales_order_id.as_ref(),
        &orders,
        actor,
        session,
    )
    .await?;
    Ok(CreatePurchaseOrdersFromSourcingResult {
        orders,
        replayed: false,
        reference: sales_order_id.to_string(),
    })
}

/// 已规范化的选源行。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestedSourcingLine {
    /// 稳定销售行。
    sales_order_line_id: String,
    /// 本行选用的供应商。
    supplier_id: String,
    /// 本次采购数量。
    quantity: Quantity,
}

/// 规范化并校验选源行。
///
/// # 参数
/// * `req` - 选源创建请求
///
/// # 返回
/// 返回稳定行去重、数量已类型化且按销售行排序的选源行。
///
/// # 错误
/// 稳定行重复、供应商空白、数量非法或数量不大于零时返回校验错误。
///
/// # 关键业务约束
/// 同一稳定销售行在一次选源命令中只能指定一家供应商。
fn normalize_sourcing_assignments(
    req: &CreatePurchaseOrdersFromSourcingRequest,
) -> Result<Vec<RequestedSourcingLine>> {
    let mut seen = HashSet::new();
    let mut lines = Vec::with_capacity(req.lines.len());
    for line in &req.lines {
        let sales_order_line_id = line.sales_order_line_id.trim().to_string();
        let supplier_id = line.supplier_id.trim().to_string();
        if sales_order_line_id.is_empty() {
            return Err(Error::ValidationError("销售行不能为空".to_string()));
        }
        if supplier_id.is_empty() {
            return Err(Error::ValidationError("供应商不能为空".to_string()));
        }
        if !seen.insert(sales_order_line_id.clone()) {
            return Err(Error::ValidationError("本次采购明细包含重复销售行".to_string()));
        }
        let quantity = Quantity::from_str(line.quantity.trim())
            .map_err(|error| Error::ValidationError(format!("本次数量非法: {error}")))?;
        let zero =
            Quantity::from_str("0").map_err(|error| Error::Internal(format!("零数量常量非法: {error}")))?;
        if quantity <= zero {
            return Err(Error::ValidationError("本次数量必须大于 0".to_string()));
        }
        lines.push(RequestedSourcingLine {
            sales_order_line_id,
            supplier_id,
            quantity,
        });
    }
    lines.sort_by(|left, right| left.sales_order_line_id.cmp(&right.sales_order_line_id));
    Ok(lines)
}

/// 把选源行归入精确依据分组，形成待创建采购单计划。
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
    groups: &[BasisGroup],
    assignments: &[RequestedSourcingLine],
) -> Result<Vec<SourcingDraftPlan>> {
    let mut plans: BTreeMap<String, SourcingDraftPlan> = BTreeMap::new();
    for assignment in assignments {
        let group = find_assignment_group(groups, assignment)?;
        let key = basis_scope_key(&group.scope);
        let requested = RequestedLine {
            sales_order_line_id: assignment.sales_order_line_id.clone(),
            quantity: assignment.quantity,
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

/// 查找一条选源行命中的精确依据。
///
/// # 参数
/// * `groups` - 当前任务范围内的精确依据
/// * `assignment` - 已规范化选源行
///
/// # 返回
/// 返回同时包含该销售行与该供应商的依据分组。
///
/// # 错误
/// 销售行不存在或供应商对该行没有合格供给时返回校验错误。
///
/// # 关键业务约束
/// 不以 SKU 猜测供给，只接受当前合格依据中已确定的供应商。
fn find_assignment_group<'a>(
    groups: &'a [BasisGroup],
    assignment: &RequestedSourcingLine,
) -> Result<&'a BasisGroup> {
    let supplier_id = SupplierAccountId::new(assignment.supplier_id.clone());
    let mut matched = None;
    for group in groups {
        let contains_line = group
            .lines
            .iter()
            .any(|line| stable_line_id(line) == assignment.sales_order_line_id);
        if !contains_line {
            continue;
        }
        if group.scope.supplier_id != supplier_id {
            continue;
        }
        if matched.is_some() {
            return Err(Error::ConflictError(
                "同一销售行对同一供应商存在多条创建依据，请刷新后重试".to_string(),
            ));
        }
        matched = Some(group);
    }
    matched.ok_or_else(|| {
        Error::ValidationError("所选供应商对当前销售行没有合格供给，或该行不在当前采购任务范围内".to_string())
    })
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
                line.supplier_id.clone(),
                line.quantity.to_string(),
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
/// * `orders` - 已提交审批的采购单
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
/// 收据与全部已提交采购单必须同事务提交。
async fn write_sourcing_receipt(
    db: &mongodb::Database,
    audit_id: &str,
    request_fingerprint: &str,
    sales_order_id: &str,
    orders: &[CreatePurchaseOrderResult],
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let receipt = SourcingReceipt {
        orders: orders
            .iter()
            .map(|order| SourcingOrderReceipt {
                purchase_order_id: order.purchase_order_id.clone(),
                purchase_no: order.purchase_no.clone(),
                lock_version: order.lock_version,
            })
            .collect(),
    };
    let audit = actor.clone().resource_log_with_id(
        audit_id.to_string(),
        CREATE_SOURCING_ACTION,
        "purchase_order",
        sales_order_id.to_string(),
        Some(command_receipt_message(request_fingerprint, &receipt)?),
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
        replayed: true,
        reference: sales_order_id.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{find_assignment_group_index, normalize_assignment_pairs, DedupError, RequestedSourcingLine};

    /// 同一销售行不能指定两次供应商。
    #[test]
    fn duplicate_sales_lines_are_rejected() {
        let error = normalize_assignment_pairs(&[("line-1", "sup-a", "1"), ("line-1", "sup-b", "1")])
            .expect_err("重复销售行必须失败");
        assert!(matches!(error, DedupError::DuplicateLine));
    }

    /// 不同销售行指定同一供应商时应归入同一分组下标。
    #[test]
    fn same_supplier_lines_share_one_group() {
        let groups = [("sup-a", &["line-1", "line-2"][..]), ("sup-b", &["line-1"][..])];
        let assignments = [line("line-1", "sup-a"), line("line-2", "sup-a")];
        let indexes = assignments
            .iter()
            .map(|assignment| find_assignment_group_index(&groups, assignment).expect("应命中供给"))
            .collect::<Vec<_>>();
        assert_eq!(indexes, vec![0, 0]);
    }

    /// 不同供应商必须拆到不同分组。
    #[test]
    fn different_suppliers_split_groups() {
        let groups = [("sup-a", &["line-1"][..]), ("sup-b", &["line-2"][..])];
        let first = find_assignment_group_index(&groups, &line("line-1", "sup-a")).expect("A");
        let second = find_assignment_group_index(&groups, &line("line-2", "sup-b")).expect("B");
        assert_ne!(first, second);
    }

    /// 销售行没有该供应商供给时不能建单。
    #[test]
    fn missing_supplier_option_is_rejected() {
        let groups = [("sup-a", &["line-1"][..])];
        assert!(find_assignment_group_index(&groups, &line("line-1", "sup-b")).is_none());
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
        assert!(production.contains("persist_basis_draft"));
    }

    /// 构造测试用选源行。
    fn line(sales_order_line_id: &str, supplier_id: &str) -> RequestedSourcingLine {
        RequestedSourcingLine {
            sales_order_line_id: sales_order_line_id.to_string(),
            supplier_id: supplier_id.to_string(),
            quantity: entities::money::Quantity::from_str("1").expect("测试数量合法"),
        }
    }
}

/// 测试辅助：选源行去重错误。
#[cfg(test)]
#[derive(Debug)]
enum DedupError {
    /// 同一销售行出现多次。
    DuplicateLine,
}

/// 测试辅助：只校验销售行去重。
///
/// # 参数
/// * `pairs` - `(销售行, 供应商, 数量)` 三元组
///
/// # 返回
/// 无重复时返回 `Ok(())`。
///
/// # 错误
/// 销售行重复时返回 `DedupError::DuplicateLine`。
#[cfg(test)]
fn normalize_assignment_pairs(pairs: &[(&str, &str, &str)]) -> std::result::Result<(), DedupError> {
    let mut seen = HashSet::new();
    for (line_id, _, _) in pairs {
        if !seen.insert(*line_id) {
            return Err(DedupError::DuplicateLine);
        }
    }
    Ok(())
}

/// 测试辅助：按供应商与销售行查找分组下标。
///
/// # 参数
/// * `groups` - `(供应商, 销售行列表)` 分组
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
    groups.iter().position(|(supplier_id, lines)| {
        *supplier_id == assignment.supplier_id
            && lines
                .iter()
                .any(|line_id| *line_id == assignment.sales_order_line_id)
    })
}
