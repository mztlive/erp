//! 将正式销售行和已归属成本归集为订单经营结果，不把缺少事实当零成本。
use super::{dto::*, source::Sources};
use crate::{Error, Result};
use erp_finance::entity::cost::{
    profit_loss::ProfitLossAmounts, CostAllocation, CostEntry, CostScope, CostStage, CostType,
};
use erp_sales::{
    entity::sales_order::FulfillmentProgress, repository::sales_order::profit_loss::ProfitLossOrder,
};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 一张销售单的可归集经营事实。
#[derive(Debug, Clone)]
pub(super) struct OrderResult {
    pub row: ProfitLossRow,
    pub revenue: Decimal,
    pub costs: ProfitLossAmounts,
    pub composition: BTreeMap<String, Decimal>,
    pub month: String,
}
impl OrderResult {
    /// 把实际金额与完整性证据同时投影，完整性由全行实际供货成本和履约完成决定。
    pub fn finish(&mut self) -> Result<()> {
        let complete = self.row.coverage_blockers.is_empty();
        self.row.coverage_state = if complete {
            "COVERED"
        } else if self.row.cost_entry_ids.is_empty() {
            "UNCOVERED"
        } else {
            "PARTIAL"
        }
        .into();
        self.row.totals = totals(
            self.revenue,
            &self.costs,
            if complete {
                Some(self.costs.profit(self.revenue)?)
            } else {
                None
            },
            self.revenue,
        );
        Ok(())
    }
}
/// 金额严格累计；异常大金额整体失败。
pub(super) fn add(target: &mut Decimal, amount: Decimal) -> Result<()> {
    *target = target
        .checked_add(amount)
        .ok_or_else(|| Error::ValidationError("统计金额超出范围".into()))?;
    Ok(())
}
/// 金额保留分，财务值已由正式行完成舍入。
pub(super) fn money(value: Decimal) -> String {
    format!("{value:.2}")
}
/// 比率按百分数展示，零分母不伪造比率。
pub(super) fn percent(numerator: Decimal, denominator: Decimal) -> Option<String> {
    if denominator <= Decimal::ZERO {
        return None;
    }
    numerator
        .checked_div(denominator)
        .and_then(|n| n.checked_mul(Decimal::from(100)))
        .map(|n| format!("{n:.2}%"))
}
/// 汇总字段只承载服务端计算结果，零收入利润仍可存在但利润率不可用。
pub(super) fn totals(
    revenue: Decimal,
    costs: &ProfitLossAmounts,
    profit: Option<Decimal>,
    profit_revenue: Decimal,
) -> Totals {
    Totals {
        net_sales_revenue: money(revenue),
        actual_procurement_cost_net: money(costs.procurement),
        actual_fulfillment_cost_net: money(costs.fulfillment),
        reductions_net: money(costs.reductions),
        actual_profit_loss_net: profit.map(money),
        margin_rate: profit.and_then(|p| percent(p, profit_revenue)),
        margin_unavailable_reason: if profit.is_none() {
            Some("成本尚未完整覆盖".into())
        } else if profit_revenue <= Decimal::ZERO {
            Some("不含税收入为零，利润率不适用".into())
        } else {
            None
        },
    }
}
/// 销售版本和明细按稳定键建立索引，避免按订单重复扫描全部事实。
struct SalesIndex<'a> {
    revisions: HashMap<&'a str, &'a erp_sales::entity::sales_order::SalesOrderRevision>,
    lines: HashMap<&'a str, Vec<&'a erp_sales::entity::sales_order::SalesOrderRevisionLine>>,
    scenarios: HashMap<&'a str, String>,
}
impl<'a> SalesIndex<'a> {
    /// 各集合只扫描一次；同一正式版本的所有行保持聚合。
    fn new(source: &'a Sources) -> Self {
        let revisions = source.revisions.iter().map(|r| (r.base.id.as_str(), r)).collect();
        let mut lines: HashMap<&str, Vec<_>> = HashMap::new();
        for line in &source.lines {
            lines
                .entry(line.sales_order_revision_id.as_ref())
                .or_default()
                .push(line);
        }
        let scenarios = source
            .goods
            .iter()
            .map(|g| {
                (
                    g.revision_line_id.as_ref(),
                    g.welfare_scenario
                        .as_ref()
                        .map(|s| s.label())
                        .unwrap_or("未标注")
                        .to_string(),
                )
            })
            .collect();
        Self {
            revisions,
            lines,
            scenarios,
        }
    }
    /// 使用当前正式版本的明细；缺失时由金额及覆盖校验拒绝可信利润。
    fn lines(
        &self,
        order: &ProfitLossOrder,
    ) -> &[&'a erp_sales::entity::sales_order::SalesOrderRevisionLine] {
        self.lines
            .get(order.current_revision_id.as_deref().unwrap_or(""))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    /// 当前版本必须属于该销售单；禁止回退到其他订单或草稿。
    fn result(&self, order: &ProfitLossOrder, drill: bool) -> Result<OrderResult> {
        let revision = self
            .revisions
            .get(order.current_revision_id.as_deref().unwrap_or(""))
            .filter(|r| r.sales_order_id.as_ref() == order.id)
            .ok_or_else(|| Error::ConflictError("销售单缺少匹配的正式版本，无法生成盈亏".into()))?;
        let mut revenue = Decimal::ZERO;
        let mut scenarios = BTreeSet::new();
        for line in self.lines(order) {
            add(&mut revenue, line.net_amount.to_decimal())?;
            scenarios.insert(
                self.scenarios
                    .get(line.base.id.as_str())
                    .cloned()
                    .unwrap_or_else(|| "未标注".into()),
            );
        }
        if revenue != revision.net_amount.to_decimal() {
            return Err(Error::ConflictError("销售版本表头与明细收入不一致".into()));
        }
        let date = order
            .effective_at
            .ok_or_else(|| Error::ConflictError("销售单缺少生效日期".into()))?
            .as_utc();
        Ok(OrderResult {
            row: order_row(order, &revision.customer_snapshot.customer_name, scenarios, drill),
            revenue,
            costs: ProfitLossAmounts::default(),
            composition: BTreeMap::new(),
            month: (date + chrono::Duration::hours(8)).format("%Y-%m").to_string(),
        })
    }
}
/// 订单展示身份只来自正式客户快照和销售单号。
fn order_row(
    order: &ProfitLossOrder,
    customer: &str,
    scenarios: BTreeSet<String>,
    drill: bool,
) -> ProfitLossRow {
    ProfitLossRow {
        row_id: order.id.clone(),
        object_type: "sales_order".into(),
        object_id: Some(order.id.clone()),
        identity_label: order.order_no.clone(),
        customer_id: Some(order.customer_id.clone()),
        customer_label: Some(customer.into()),
        benefit_scenarios: scenarios.into_iter().collect(),
        fulfillment_modes: vec![],
        totals: Totals::default(),
        coverage_state: String::new(),
        coverage_blockers: vec![],
        latest_cost_occurred_at: None,
        allowed_drilldowns: if drill { vec!["cost_entry".into()] } else { vec![] },
        cost_entry_ids: vec![],
    }
}
/// 按订单和版本索引归集，数据库读取次数及内存遍历不随单据数平方增长。
pub(super) fn calculate(source: &Sources, as_of: i64, drill: bool) -> Result<Vec<OrderResult>> {
    let sales = SalesIndex::new(source);
    let entries: HashMap<_, _> = source.entries.iter().map(|e| (e.base.id.as_str(), e)).collect();
    let mut allocations: HashMap<&str, Vec<&CostAllocation>> = HashMap::new();
    for a in &source.allocations {
        if let Some(id) = &a.sales_order_id {
            allocations.entry(id.as_ref()).or_default().push(a);
        }
    }
    source
        .orders
        .iter()
        .map(|order| {
            let mut result = sales.result(order, drill)?;
            let mut covered_lines = BTreeSet::new();
            for allocation in allocations.get(order.id.as_str()).into_iter().flatten() {
                apply_cost(&mut result, allocation, &entries, as_of, &mut covered_lines)?;
            }
            check_coverage(&mut result, sales.lines(order), order, &covered_lines)?;
            result.finish()?;
            Ok(result)
        })
        .collect()
}
/// 只消费非卡券范围的分配金额；未来发生的事实不进入查询时点。
fn apply_cost(
    result: &mut OrderResult,
    allocation: &CostAllocation,
    entries: &HashMap<&str, &CostEntry>,
    as_of: i64,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    let Some(entry) = entries.get(allocation.cost_entry_id.as_ref()) else {
        blocker(result, "COST_FACT_MISSING", "存在分配但成本事实缺失，请财务核查");
        return Ok(());
    };
    if entry.cost_scope != CostScope::NonVoucherFulfillment || entry.occurred_at.as_utc().timestamp() > as_of
    {
        return Ok(());
    }
    let amount = allocation.allocated_net_amount.to_decimal();
    result.costs.add(entry.cost_stage, entry.cost_type, amount)?;
    if !entry.cost_stage.is_profit_relevant() {
        return Ok(());
    }
    record_actual_cost(result, allocation, entry, amount, covered)
}
/// 保存已发生费用的来源及净费用构成，冲减只反向一次。
fn record_actual_cost(
    result: &mut OrderResult,
    allocation: &CostAllocation,
    entry: &CostEntry,
    amount: Decimal,
    covered: &mut BTreeSet<String>,
) -> Result<()> {
    record_cost_source(&mut result.row, entry);
    let signed = if entry.cost_stage == CostStage::Reduction {
        -amount
    } else {
        amount
    };
    add(
        result
            .composition
            .entry(entry.cost_type.as_str().into())
            .or_default(),
        signed,
    )?;
    if entry.cost_stage == CostStage::Actual
        && matches!(entry.cost_type, CostType::Product | CostType::OfflineService)
    {
        covered.insert(
            allocation
                .sales_order_line_id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        );
    }
    Ok(())
}
/// 原始费用来源按稳定身份去重；发生日期只读取正式事实。
fn record_cost_source(row: &mut ProfitLossRow, entry: &CostEntry) {
    if !row.cost_entry_ids.contains(&entry.base.id) {
        row.cost_entry_ids.push(entry.base.id.clone());
    }
    let time = entry.occurred_at.as_utc().to_rfc3339();
    if row.latest_cost_occurred_at.as_ref().is_none_or(|t| t < &time) {
        row.latest_cost_occurred_at = Some(time);
    }
}
/// 覆盖率为整单收入覆盖；部分交付和缺任何销售行供货成本均不得宣称整单利润。
fn check_coverage(
    result: &mut OrderResult,
    lines: &[&erp_sales::entity::sales_order::SalesOrderRevisionLine],
    order: &ProfitLossOrder,
    covered: &BTreeSet<String>,
) -> Result<()> {
    let current: BTreeSet<_> = lines.iter().map(|l| l.sales_order_line_id.as_ref()).collect();
    let evidence = erp_finance::entity::cost::profit_loss::CoverageEvidence {
        fulfillment_completed: order.fulfillment_progress == FulfillmentProgress::Completed,
        current_lines: current,
        supplied_lines: covered,
    };
    for gap in result.costs.coverage_gaps(evidence)? {
        blocker(result, gap.code, &gap.message);
    }
    Ok(())
}
/// 相同缺口仅展示一次。
fn blocker(result: &mut OrderResult, code: &str, message: &str) {
    if !result.row.coverage_blockers.iter().any(|b| b.code == code) {
        result.row.coverage_blockers.push(CoverageBlocker {
            code: code.into(),
            message: message.into(),
        });
    }
}
