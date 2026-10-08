//! 采购命令重验与创建依据视图共用的数据来源；所有读取使用调用方 Executor。
use std::collections::{HashMap, HashSet};

use erp_core::ids::SalesOrderId;
use erp_inventory::InventoryExt;
use erp_procurement::entity::purchase_order::{
    BasisGroup, CreationBasisFacts, SalesProcurementCoverage, StockBasisGroup,
};
use erp_procurement::service::purchase_order::creation_basis::{
    basis_groups_from_facts, physical_stock_lines, stock_groups_from_facts, zero_quantity,
};
use erp_sales::entity::sales_order::{CommercialStatus, SalesOrder};
use erp_sales::repository::SalesOrderExt;
use erp_warehouse::WarehouseExt;
use persistence_core::Executor;

use super::mapping::{sales_order_basis_fact, stock_balance_fact};
use super::{load_creation_basis_facts, load_sales_procurement_coverage};
use crate::{Error, Result};
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
    Ok(basis_groups_and_facts(db, order, responsibility_scope_ids, executor).await?.0)
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
    basis_groups_and_facts_from_coverage(db, order, responsibility_scope_ids, &coverage, executor).await
}

/// 已读取的同阶段覆盖只用于当前依据组装，不能替代 guard 后或库存写入后的重验。
async fn basis_groups_and_facts_from_coverage(
    db: &mongodb::Database,
    order: &SalesOrder,
    responsibility_scope_ids: &[String],
    coverage: &SalesProcurementCoverage,
    executor: &mut dyn Executor,
) -> Result<(Vec<BasisGroup>, CreationBasisFacts)> {
    let facts = creation_basis_facts_for_order(db, coverage, responsibility_scope_ids, executor).await?;
    let groups =
        basis_groups_from_facts(&sales_order_basis_fact(order), coverage, responsibility_scope_ids, &facts)?;
    Ok((groups, facts))
}

/// 同一读取阶段按原顺序形成采购与现有库存依据，共用一次完整覆盖读取。
/// # 参数
/// 数据库、已生效销售单、任务责任行及调用方事务执行器。
/// # 返回
/// 精确采购依据和库存依据；非生效销售单返回两份空集合。
/// # 错误
/// 覆盖、采购供给、库存或仓库读取失败时按采购先于库存的顺序返回错误。
///
/// 只适用于两份依据之间没有业务写入的读取阶段；guard 后及库存写入后须重新读取。
pub async fn sourcing_groups_for_order(
    db: &mongodb::Database,
    order: &SalesOrder,
    responsibility_scope_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<(Vec<BasisGroup>, Vec<StockBasisGroup>)> {
    if order.commercial_status != CommercialStatus::Effective {
        return Ok((Vec::new(), Vec::new()));
    }
    let coverage = load_sales_procurement_coverage(db, order, executor).await?;
    let (groups, _) =
        basis_groups_and_facts_from_coverage(db, order, responsibility_scope_ids, &coverage, executor)
            .await?;
    let stock = stock_groups_from_coverage(db, responsibility_scope_ids, &coverage, executor).await?;
    Ok((groups, stock))
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
    let scope = responsibility_scope_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let sku_ids = coverage
        .lines
        .iter()
        .filter(|line| {
            scope.contains(line.revision_line.sales_order_line_id.as_ref())
                && line.summary.remaining_quantity > zero_quantity()
        })
        .map(|line| line.goods_line.sku_id.clone())
        .collect::<Vec<_>>();
    load_creation_basis_facts(db, &sku_ids, executor).await.map_err(Into::into)
}

/// 由销售当前版本、统一覆盖与公司可用库存形成现有库存供给依据。
///
/// # 参数
/// * `db` - 目标数据库。
/// * `order` - 销售单。
/// * `responsibility_scope_ids` - 任务冻结的销售行身份。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 销售单未生效时返回空集合；否则返回现有库存供给依据分组。
///
/// # 错误
/// 采购覆盖、可用库存或仓库读取失败时返回对应错误。
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
    stock_groups_from_coverage(db, responsibility_scope_ids, &coverage, executor).await
}

/// 仅从原阶段覆盖补充库存和仓库事实，不重新装载销售与采购覆盖。
async fn stock_groups_from_coverage(
    db: &mongodb::Database,
    responsibility_scope_ids: &[String],
    coverage: &SalesProcurementCoverage,
    executor: &mut dyn Executor,
) -> Result<Vec<StockBasisGroup>> {
    let physical_lines = physical_stock_lines(coverage, responsibility_scope_ids);
    let sku_ids = physical_lines.iter().map(|line| line.goods_line.sku_id.clone()).collect::<Vec<_>>();
    let balances = db.inventory().available_balances_for_skus(&sku_ids, executor).await?;
    let warehouse_ids = balances
        .iter()
        .map(|balance| balance.warehouse_id.to_string())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let warehouses = db.warehouses().list_active_by_ids(&warehouse_ids, executor).await?;
    let active_warehouses =
        warehouses.into_iter().filter(|warehouse| warehouse.is_active()).collect::<Vec<_>>();
    let active_warehouse_ids =
        active_warehouses.iter().map(|warehouse| warehouse.base.id.clone()).collect::<HashSet<_>>();
    let revision_ids = active_warehouses
        .iter()
        .filter_map(|warehouse| warehouse.stable.current_revision_id.clone())
        .collect::<Vec<_>>();
    let revisions = db.warehouse_revisions().list_active_by_ids(&revision_ids, executor).await?;
    let names = active_warehouses
        .into_iter()
        .map(|warehouse| {
            let name = warehouse
                .stable
                .current_revision_id
                .as_deref()
                .and_then(|revision_id| revisions.iter().find(|revision| revision.base.id == revision_id))
                .map(|revision| revision.name.as_str());
            let name = warehouse_display(name, &warehouse.warehouse_code);
            (warehouse.base.id, name)
        })
        .collect::<HashMap<_, _>>();
    Ok(stock_groups_from_facts(
        coverage,
        &physical_lines,
        balances.into_iter().map(stock_balance_fact).collect(),
        &active_warehouse_ids,
        &names,
    ))
}

/// 库存依据以仓库名称展示；缺少当前修订时使用稳定业务编号。
fn warehouse_display(name: Option<&str>, warehouse_code: &str) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| Some(warehouse_code.trim()).filter(|code| !code.is_empty()))
        .unwrap_or("仓库名称不可用")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::warehouse_display;

    #[test]
    fn warehouse_basis_prefers_name_and_falls_back_to_business_code() {
        assert_eq!(warehouse_display(Some("  北京通州仓  "), "WH-BJ-001"), "北京通州仓");
        assert_eq!(warehouse_display(None, "WH-BJ-001"), "WH-BJ-001");
        assert_eq!(warehouse_display(Some("  "), "  WH-BJ-001  "), "WH-BJ-001");
        assert_eq!(warehouse_display(None, "  "), "仓库名称不可用");
    }
}
