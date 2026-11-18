use erp_core::money::Quantity;

use super::super::command_receipt::digest_parts;
use super::super::coverage::SalesProcurementCoverageLine;
use crate::entity::facts::{
    SalesOrderBasisFact as SalesOrder, SalesRevisionFact as SalesOrderRevision,
    StockBalanceFact as StockBalance,
};

/// 一条可由现有库存直接满足的销售行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBasisLine {
    /// 当前销售版本行及统一供给覆盖摘要。
    pub coverage: SalesProcurementCoverageLine,
    /// 本余额本次最多可分配数量。
    pub max_create_quantity: Quantity,
}

impl StockBasisLine {
    // 判断本行是否覆盖指定稳定销售行。覆盖行匹配时返回 `true`。
    fn covers(&self, sales_order_line_id: &str) -> bool {
        self.coverage.revision_line.sales_order_line_id.as_ref() == sales_order_line_id
    }
}

/// 一个仓库库存余额形成的现有库存供给依据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBasisGroup {
    /// 销售当前版本。
    pub revision: SalesOrderRevision,
    /// 被分配的库存余额。
    pub balance: StockBalance,
    /// 仓库当前名称；基础资料缺失时回退仓库 ID。
    pub warehouse_name: String,
    /// 该余额可满足的销售行。
    pub lines: Vec<StockBasisLine>,
}

impl StockBasisGroup {
    /// 查找余额依据中的稳定销售行。命中时返回该行，否则返回 `None`。
    pub fn line_for(&self, sales_order_line_id: &str) -> Option<&StockBasisLine> {
        self.lines.iter().find(|line| line.covers(sales_order_line_id))
    }
}

/// 形成绑定销售 guard、库存余额版本与逐行剩余量的现有库存依据 ID。
///
/// guard 每次成功创建后推进，使作废释放的剩余量可形成新依据；余额版本、可用量与逐行剩余量任一变化都会改变依据身份。
///
/// 返回 `{sales_order_id}:{sha256}` 稳定依据 ID。
///
/// - `order`：销售稳定单。
/// - `work_item_id`：冻结本依据责任范围的开放任务。
pub fn stock_basis_id_for(order: &SalesOrder, group: &StockBasisGroup, work_item_id: &str) -> String {
    let mut parts = vec![
        order.base.id.clone(),
        work_item_id.to_string(),
        order.procurement_guard_version.to_string(),
        group.revision.base.id.clone(),
        group.balance.base.id.clone(),
        group.balance.base.version.to_string(),
        group.balance.available_quantity.to_string(),
    ];
    parts.extend(group.lines.iter().map(|line| {
        format!(
            "{}|{}|{}|{}",
            line.coverage.revision_line.sales_order_line_id,
            line.coverage.revision_line.base.id,
            line.coverage.summary.remaining_quantity,
            line.max_create_quantity,
        )
    }));

    format!("{}:{}", order.base.id, digest_parts(parts))
}

/// 已归入一个库存余额的现有库存分配计划。
#[derive(Debug, Clone)]
pub struct StockAllocationPlan {
    /// 命中的现有库存依据。
    pub group: StockBasisGroup,
    /// 本余额逐销售行分配数量。
    pub requested_lines: Vec<RequestedStockLine>,
}

/// 已规范化的现有库存分配行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedStockLine {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 本次预占数量。
    pub quantity: Quantity,
}
