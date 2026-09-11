//! 全部匹配订单共用筛选、分组、汇总和导出投影；分页最后执行。
use super::{
    calculation::{add, money, percent, totals, OrderResult},
    dto::*,
    query::{cost_types, BASIS_LABEL, FORMULA_VERSION},
};
use crate::Result;
use erp_finance::entity::cost::{profit_loss::ProfitLossAmounts, CostStage, CostType};
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

/// 多维汇总中保留的可靠子集及全部已知成本。
#[derive(Default)]
struct Summary {
    revenue: Decimal,
    covered: Decimal,
    profit: Decimal,
    covered_count: usize,
    costs: ProfitLossAmounts,
    composition: BTreeMap<String, Decimal>,
}
impl Summary {
    /// 同一订单只累计一次，成本类别筛选不截断单内成本。
    fn push(&mut self, order: &OrderResult) -> Result<()> {
        add(&mut self.revenue, order.revenue)?;
        merge_costs(&mut self.costs, &order.costs)?;
        for (kind, amount) in &order.composition {
            add(self.composition.entry(kind.clone()).or_default(), *amount)?;
        }
        if order.row.coverage_state == "COVERED" {
            self.covered_count += 1;
            add(&mut self.covered, order.revenue)?;
            add(&mut self.profit, order.costs.profit(order.revenue)?)?;
        }
        Ok(())
    }
    /// 空集合没有可依赖的利润结果。
    fn profit(&self) -> Option<Decimal> {
        (self.covered_count > 0).then_some(self.profit)
    }
    /// 全量覆盖结果不依赖当前明细分页。
    fn coverage(&self, count: usize) -> Coverage {
        let complete = count > 0 && self.covered_count == count;
        Coverage {
            covered_net_revenue: money(self.covered),
            uncovered_net_revenue: money(self.revenue - self.covered),
            coverage_rate: percent(self.covered, self.revenue).unwrap_or_else(|| "不适用".into()),
            reliability: if complete {
                "reliable"
            } else if self.covered_count > 0 {
                "partial"
            } else {
                "unavailable"
            }
            .into(),
            coverage_state: if complete {
                "complete"
            } else if self.covered_count > 0 {
                "partial"
            } else {
                "none"
            }
            .into(),
        }
    }
}
/// 复用领域阶段累加规则，避免把确认成本带入实际利润。
fn merge_costs(target: &mut ProfitLossAmounts, source: &ProfitLossAmounts) -> Result<()> {
    for (stage, kind, value) in [
        (CostStage::Actual, CostType::Product, source.procurement),
        (CostStage::Actual, CostType::Other, source.fulfillment),
        (CostStage::Reduction, CostType::Other, source.reductions),
        (
            CostStage::Expected,
            CostType::Product,
            source.expected_procurement,
        ),
        (CostStage::Expected, CostType::Other, source.expected_fulfillment),
        (
            CostStage::Confirmed,
            CostType::Product,
            source.confirmed_procurement,
        ),
        (
            CostStage::Confirmed,
            CostType::Other,
            source.confirmed_fulfillment,
        ),
    ] {
        target.add(stage, kind, value)?;
    }
    Ok(())
}
/// 先应用对象筛选，覆盖率保留所选期间中完整和不完整订单的分母。
pub(super) fn project(
    orders: Vec<OrderResult>,
    query: &ProfitLossQuery,
    as_of: &str,
    export: bool,
    scope_label: &str,
) -> Result<ProfitLossView> {
    let matched: Vec<_> = orders.into_iter().filter(|o| matches_query(o, query)).collect();
    let coverage = summarize(&matched)?.coverage(matched.len());
    let selected: Vec<_> = matched
        .into_iter()
        .filter(|o| coverage_matches(o, &query.coverage))
        .collect();
    let summary = summarize(&selected)?;
    let trend = trend(&selected)?;
    let rows = rows(&selected, query, export)?;
    assemble(query, as_of, scope_label, coverage, summary, trend, rows)
}
/// 从同一订单集合计算所有总额。
fn summarize(orders: &[OrderResult]) -> Result<Summary> {
    let mut summary = Summary::default();
    for order in orders {
        summary.push(order)?;
    }
    Ok(summary)
}
/// 覆盖筛选只决定所选订单，不改变覆盖率分母。
fn coverage_matches(order: &OrderResult, coverage: &str) -> bool {
    match coverage {
        "covered" => order.row.coverage_state == "COVERED",
        "uncovered" => order.row.coverage_state != "COVERED",
        _ => true,
    }
}
/// 排序和分页最后执行；导出保留全部匹配分组。
fn rows(orders: &[OrderResult], query: &ProfitLossQuery, export: bool) -> Result<Rows> {
    let mut items = group_rows(orders, &query.dimension)?;
    items.sort_by(|a, b| compare_rows(a, b, &query.sort));
    let total = items.len();
    if !export {
        items = items
            .into_iter()
            .skip((query.page - 1) * query.page_size)
            .take(query.page_size)
            .collect();
    }
    Ok(Rows {
        dimension: query.dimension.clone(),
        items,
        total,
    })
}
/// 搜索为销售单号与正式客户快照的字面量包含；场景及成本类型按整单命中。
fn matches_query(order: &OrderResult, query: &ProfitLossQuery) -> bool {
    let keyword = query.q.as_deref().unwrap_or("").trim().to_lowercase();
    let text = format!(
        "{} {}",
        order.row.identity_label,
        order.row.customer_label.as_deref().unwrap_or("")
    )
    .to_lowercase();
    text.contains(&keyword)
        && query
            .benefit_scenario
            .as_ref()
            .is_none_or(|s| order.row.benefit_scenarios.contains(s))
        && (query.cost_codes().is_empty()
            || query
                .cost_codes()
                .iter()
                .any(|t| order.composition.contains_key(*t)))
}
/// 只有销售单和客户身份允许链接；多场景订单作为一个组合分组，不重复收入。
fn group_rows(orders: &[OrderResult], dimension: &str) -> Result<Vec<ProfitLossRow>> {
    if dimension == "sales_order" {
        return Ok(orders.iter().map(|o| o.row.clone()).collect());
    }
    let mut groups: BTreeMap<String, Vec<&OrderResult>> = BTreeMap::new();
    for order in orders {
        let key = if dimension == "customer" {
            order.row.customer_id.clone().unwrap_or_default()
        } else if order.row.benefit_scenarios.is_empty() {
            "未标注".into()
        } else {
            order.row.benefit_scenarios.join("、")
        };
        groups.entry(key).or_default().push(order);
    }
    groups
        .into_iter()
        .map(|(key, values)| group_row(&key, &values, dimension))
        .collect()
}
/// 分组完整性要求组内每单完整；混合组不输出完整利润。
fn group_row(key: &str, values: &[&OrderResult], dimension: &str) -> Result<ProfitLossRow> {
    let mut summary = Summary::default();
    for order in values {
        summary.push(order)?;
    }
    let mut row = values[0].row.clone();
    row.row_id = format!("{dimension}:{key}");
    row.object_type = dimension.into();
    row.object_id = (dimension == "customer").then(|| key.into());
    row.identity_label = if dimension == "customer" {
        row.customer_label.clone().unwrap_or_else(|| "客户".into())
    } else {
        key.into()
    };
    if dimension != "customer" {
        row.customer_id = None;
        row.customer_label = None;
    }
    group_totals(&mut row, &summary, values.len());
    group_sources(&mut row, values);
    Ok(row)
}
/// 分组中任一订单未覆盖，整组完整利润即不可用。
fn group_totals(row: &mut ProfitLossRow, summary: &Summary, count: usize) {
    let complete = summary.covered_count == count;
    row.totals = totals(
        summary.revenue,
        &summary.costs,
        complete.then_some(summary.profit),
        summary.revenue,
    );
    row.coverage_state = if complete {
        "COVERED"
    } else if summary.covered_count > 0 {
        "PARTIAL"
    } else {
        "UNCOVERED"
    }
    .into();
    row.coverage_blockers = if complete {
        vec![]
    } else {
        vec![CoverageBlocker {
            code: "GROUP_INCOMPLETE".into(),
            message: format!("{} 单成本尚未完整覆盖", count - summary.covered_count),
        }]
    };
}
/// 分组来源去重，最新成本时间取组内最新实际事实。
fn group_sources(row: &mut ProfitLossRow, values: &[&OrderResult]) {
    row.cost_entry_ids = values
        .iter()
        .flat_map(|o| o.row.cost_entry_ids.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    row.benefit_scenarios = values
        .iter()
        .flat_map(|o| o.row.benefit_scenarios.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    row.latest_cost_occurred_at = values
        .iter()
        .filter_map(|o| o.row.latest_cost_occurred_at.clone())
        .max();
}
/// 趋势遵循同一筛选；横轴为销售单生效月份，金额为查询时点累计结果。
fn trend(orders: &[OrderResult]) -> Result<Vec<TrendPoint>> {
    let mut months: BTreeMap<&str, (Summary, usize)> = BTreeMap::new();
    for order in orders {
        let (summary, count) = months.entry(&order.month).or_default();
        summary.push(order)?;
        *count += 1;
    }
    months
        .into_iter()
        .map(|(month, (s, count))| {
            Ok(TrendPoint {
                period: month.into(),
                net_sales_revenue: money(s.revenue),
                actual_cost_net: money(s.costs.net_cost()?),
                actual_profit_loss_net: s.profit().map(money),
                reliability: s.coverage(count).reliability,
            })
        })
        .collect()
}
/// 页内顺序与导出一致，空利润在两个方向都排在末尾，同值按稳定键排序。
fn compare_rows(a: &ProfitLossRow, b: &ProfitLossRow, sort: &str) -> Ordering {
    let (field, direction) = sort.split_once(':').unwrap_or(("actualProfitLossNet", "asc"));
    let mut ordering = match field {
        "identityLabel" => a.identity_label.cmp(&b.identity_label),
        "coverageState" => a.coverage_state.cmp(&b.coverage_state),
        _ => match (numeric(a, field), numeric(b, field)) {
            (Some(a), Some(b)) => a.cmp(&b),
            (None, Some(_)) => return Ordering::Greater,
            (Some(_), None) => return Ordering::Less,
            _ => Ordering::Equal,
        },
    };
    if direction == "desc" {
        ordering = ordering.reverse();
    }
    ordering.then_with(|| a.row_id.cmp(&b.row_id))
}
/// 字符串金额排序使用 Decimal，禁止按字典序排列负数和大金额。
fn numeric(row: &ProfitLossRow, field: &str) -> Option<Decimal> {
    let t = &row.totals;
    let value = match field {
        "netSalesRevenue" => Some(&t.net_sales_revenue),
        "actualProcurementCostNet" => Some(&t.actual_procurement_cost_net),
        "actualFulfillmentCostNet" => Some(&t.actual_fulfillment_cost_net),
        "reductionsNet" => Some(&t.reductions_net),
        "marginRate" => t.margin_rate.as_ref(),
        _ => t.actual_profit_loss_net.as_ref(),
    }?;
    Decimal::from_str(value.trim_end_matches('%')).ok()
}
/// 页面主合同只装配各独立投影，金额与字段来源不在 HTTP 层重算。
fn assemble(
    q: &ProfitLossQuery,
    as_of: &str,
    scope_label: &str,
    coverage: Coverage,
    s: Summary,
    trend: Vec<TrendPoint>,
    rows: Rows,
) -> Result<ProfitLossView> {
    Ok(ProfitLossView {
        scope: scope(scope_label),
        period: period(q),
        business_type: "GOODS_SERVICE".into(),
        amount_basis: "NET".into(),
        amount_basis_label: "不含税".into(),
        business_type_label: "非卡券".into(),
        formula_version: FORMULA_VERSION.into(),
        formula_text: FORMULA.into(),
        freshness: freshness(as_of),
        totals: totals(s.revenue, &s.costs, s.profit(), s.covered),
        coverage,
        field_permissions: permissions(),
        trend,
        cost_composition: composition(&s)?,
        stage_reference: stages(&s.costs)?,
        rows,
        filter_summary: filter_summary(q),
        excluded_note: EXCLUDED.into(),
    })
}
const FORMULA: &str = "实际经营盈亏 = 不含税销售收入 − 实际采购成本 − 实际履约费用 + 成本冲减。利润和利润率仅汇总成本完整订单；收入、已登记成本同时展示所选订单全量。";
const EXCLUDED: &str = "卡券不进入本表。按销售单首次生效日选择订单，使用当前正式销售版本及查询时点已发生的累计成本，并非期间现金收支或财务月结。完整覆盖要求履约完成且每条销售明细有实际供货成本；未归属成本、预计与确认成本不代替实际成本。后补费用将改变历史订单结果。";
/// 当前权限范围不是客户端 scope_id。
fn scope(label: &str) -> Scope {
    Scope {
        id: "authorized".into(),
        label: label.into(),
        permission_version: "current-rbac".into(),
    }
}
/// 自然日与口径同时导出。
fn period(q: &ProfitLossQuery) -> Period {
    Period {
        from: q.from.clone(),
        to: q.to.clone(),
        basis: q.period_basis.clone(),
        basis_label: BASIS_LABEL.into(),
        timezone: "Asia/Shanghai".into(),
    }
}
/// 即时快照没有异步投影延迟；时点由服务端提供。
fn freshness(as_of: &str) -> Freshness {
    Freshness {
        projected_at: as_of.into(),
        source_watermark: as_of.into(),
        state: "fresh".into(),
    }
}
/// 入口必须同时验证销售与成本查询权限后才调用分析。
fn permissions() -> FieldPermissions {
    FieldPermissions {
        can_view_revenue: true,
        can_view_cost: true,
        can_view_profit: true,
        can_export: true,
    }
}
/// 费用构成使用分配后金额，并保留全部费用类型选项。
fn composition(s: &Summary) -> Result<Vec<CostComposition>> {
    let total = s.costs.net_cost()?;
    Ok(cost_types()
        .into_iter()
        .map(|t| {
            let amount = s.composition.get(t.as_str()).copied().unwrap_or_default();
            CostComposition {
                cost_type: t.as_str().into(),
                label: t.label().into(),
                net_amount: money(amount),
                share: percent(amount, total).unwrap_or_else(|| "不适用".into()),
            }
        })
        .collect())
}
/// 两种非实际阶段只提供对照，不计入实际盈亏。
fn stages(costs: &ProfitLossAmounts) -> Result<Vec<StageReference>> {
    Ok(vec![
        stage(
            "EXPECTED",
            "预计",
            costs.expected_procurement,
            costs.expected_fulfillment,
        )?,
        stage(
            "CONFIRMED",
            "确认",
            costs.confirmed_procurement,
            costs.confirmed_fulfillment,
        )?,
    ])
}
/// 阶段汇总同样执行溢出检查。
fn stage(code: &str, label: &str, procurement: Decimal, fulfillment: Decimal) -> Result<StageReference> {
    let mut total = procurement;
    add(&mut total, fulfillment)?;
    Ok(StageReference {
        stage: code.into(),
        label: label.into(),
        procurement_cost_net: money(procurement),
        fulfillment_cost_net: money(fulfillment),
        total_net: money(total),
        note: "仅展示已分配到所选销售单的阶段成本，不进入实际盈亏".into(),
    })
}
/// 导出必须保留所有已应用筛选，内部身份只作为精确筛选追溯值。
fn filter_summary(q: &ProfitLossQuery) -> String {
    let coverage = match q.coverage.as_str() {
        "covered" => "成本完整",
        "uncovered" => "未完整覆盖",
        _ => "全部",
    };
    let dimension = match q.dimension.as_str() {
        "customer" => "客户",
        "scenario" => "福利场景组合",
        _ => "销售单",
    };
    let types = cost_types()
        .iter()
        .filter(|t| q.cost_codes().contains(&t.as_str()))
        .map(|t| t.label())
        .collect::<Vec<_>>()
        .join("、");
    let mut parts = vec![
        format!("{} 至 {}", q.from, q.to),
        BASIS_LABEL.into(),
        format!("覆盖：{coverage}"),
        format!("分组：{dimension}"),
    ];
    append_filters(&mut parts, q, &types);
    parts.join("；")
}

/// 附加已应用的业务筛选，费用类别匹配整单。
fn append_filters(parts: &mut Vec<String>, q: &ProfitLossQuery, types: &str) {
    if let Some(value) = &q.q {
        parts.push(format!("搜索：{value}"));
    }
    if let Some(value) = &q.benefit_scenario {
        parts.push(format!("场景：{value}"));
    }
    if !types.is_empty() {
        parts.push(format!("含费用类型：{types}"));
    }
    if q.customer_id.is_some() {
        parts.push("已限定客户".into());
    }
    if q.sales_order_id.is_some() {
        parts.push("已限定销售单".into());
    }
}
