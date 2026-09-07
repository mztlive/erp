//! 采购命令重验与创建依据视图共用的数据来源；所有读取使用调用方 Executor。
use super::mapping::{sales_order_basis_fact, stock_balance_fact};
use super::{load_creation_basis_facts, load_sales_procurement_coverage};
use erp_core::ids::SalesOrderId;
use erp_inventory::InventoryExt;
use erp_procurement::entity::purchase_order::{
    BasisGroup, CreationBasisFacts, SalesProcurementCoverage, StockBasisGroup,
};
use erp_procurement::service::purchase_order::creation_basis::{
    basis_groups_from_facts, physical_stock_lines, stock_groups_from_facts, zero_quantity,
};
use erp_sales::{
    entity::sales_order::{CommercialStatus, SalesOrder},
    repository::SalesOrderExt,
};
use erp_warehouse::WarehouseExt;
use persistence_core::Executor;
use services::{Error, Result};
use std::collections::{HashMap, HashSet};
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
    let groups = basis_groups_from_facts(
        &sales_order_basis_fact(order),
        &coverage,
        responsibility_scope_ids,
        &facts,
    )?;
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
    load_creation_basis_facts(db, &sku_ids, executor)
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
    let coverage = load_sales_procurement_coverage(db, order, executor).await?;
    let physical_lines = physical_stock_lines(&coverage, responsibility_scope_ids);
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
    let warehouses = db
        .warehouses()
        .list_active_by_ids(&warehouse_ids, executor)
        .await?;
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
        .warehouse_revisions()
        .list_active_by_ids(&revision_ids, executor)
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
    Ok(stock_groups_from_facts(
        &coverage,
        &physical_lines,
        balances.into_iter().map(stock_balance_fact).collect(),
        &active_warehouse_ids,
        &names,
    ))
}
