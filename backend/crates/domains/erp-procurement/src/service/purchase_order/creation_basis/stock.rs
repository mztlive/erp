//! 现有库存供给的责任范围筛选、数量上限与稳定分组。
use std::collections::{HashMap, HashSet};

use super::zero_quantity;
use crate::entity::facts::{ProductKind, StockBalanceFact};
use crate::entity::purchase_order::{
    SalesProcurementCoverage, SalesProcurementCoverageLine, StockBasisGroup, StockBasisLine,
};
/// 选取当前冻结责任范围内有剩余的实物销售行；保持原行顺序。
///
/// # 参数
/// * `coverage` - 销售当前采购覆盖
/// * `responsibility_scope_ids` - 允许进入库存依据的稳定销售行 ID
///
/// # 返回
/// 返回商品类型为实物、行 ID 在范围内且剩余数量大于零的覆盖行。
///
/// # 错误
/// 不返回错误。
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
///
/// 只保留启用仓库中、且至少有一条 SKU 相同的实物行的余额。行的可建数量取剩余量与可用量的较小值。
/// 仓库名缺失时回退仓库 ID。
///
/// # 参数
/// * `coverage` - 提供销售修订头，写入每个库存组
/// * `physical_lines` - 已按责任范围筛过的实物覆盖行
/// * `balances` - 最新库存余额
/// * `active_warehouse_ids` - 启用仓库 ID
/// * `names` - 仓库 ID 到名称
///
/// # 返回
/// 返回按余额 ID 排序的库存依据组；组内行按稳定销售行 ID 排序。
///
/// # 错误
/// 不返回错误。
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
