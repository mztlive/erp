//! 现有库存供给的责任范围筛选、数量上限与稳定分组。
use std::collections::{HashMap, HashSet};

use super::zero_quantity;
use crate::entity::facts::{ProductKind, StockBalanceFact};
use crate::entity::purchase_order::{
    SalesProcurementCoverage, SalesProcurementCoverageLine, StockBasisGroup, StockBasisLine,
};
/// 选取当前冻结责任范围内有剩余的实物销售行；保持原行顺序。
pub fn physical_stock_lines(
    coverage: &SalesProcurementCoverage,
    responsibility_scope_ids: &[String],
) -> Vec<SalesProcurementCoverageLine> {
    let scope = responsibility_scope_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    coverage
        .lines
        .iter()
        .filter(|line| {
            line.product_kind == ProductKind::Physical
                && scope.contains(line.revision_line.sales_order_line_id.as_ref())
                && line.summary.remaining_quantity > zero_quantity()
        })
        .cloned()
        .collect::<Vec<_>>()
}
/// 按最新余额与启用仓库形成库存依据，行按稳定销售行、组按余额身份排序。
pub fn stock_groups_from_facts(
    coverage: &SalesProcurementCoverage,
    physical_lines: &[SalesProcurementCoverageLine],
    balances: Vec<StockBalanceFact>,
    active_warehouse_ids: &HashSet<String>,
    names: &HashMap<String, String>,
) -> Vec<StockBasisGroup> {
    let mut groups = balances
        .into_iter()
        .filter(|balance| active_warehouse_ids.contains(balance.warehouse_id.as_ref()))
        .filter_map(|balance| {
            let lines = physical_lines
                .iter()
                .filter(|line| line.goods_line.sku_id == balance.sku_id)
                .cloned()
                .map(|coverage| StockBasisLine {
                    max_create_quantity: coverage.summary.remaining_quantity.min(balance.available_quantity),
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
    groups
}
