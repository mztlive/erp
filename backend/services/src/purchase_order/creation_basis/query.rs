use std::collections::{HashMap, HashSet};

use database::{InventoryExt, PurchaseOrderExt, SalesOrderExt, WorkItemExt};
use entities::catalog::ProductKind;
use entities::purchase_order::{
    BasisGroup, CreationBasisFacts, SalesProcurementCoverage, StockBasisGroup, StockBasisLine,
};
use entities::sales_order::{CommercialStatus, SalesOrder};
use erp_core::ids::{SalesOrderId, SkuId};
use persistence_core::{Executor, NoTransaction};

use super::super::coverage::load_sales_procurement_coverage;
use super::super::dto::{CreationBasisListParams, CreationBasisView};
use super::super::PurchaseOrderService;
use super::mapping::{basis_groups_from_facts, build_basis_view, build_stock_basis_view, zero_quantity};
use crate::errors::{Error, Result};
use application_core::AuditActor;

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
            .sales_orders()
            .find_effective_orders_by_ids(&order_ids, &mut NoTransaction)
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
pub async fn load_effective_sales_order(
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
pub async fn basis_groups_for_order(
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
pub async fn basis_groups_and_facts(
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

/// 由销售当前版本、统一覆盖与公司可用库存形成现有库存供给依据。
pub async fn stock_basis_groups_for_order(
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
