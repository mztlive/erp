//! 历史贡献口径：冻结归属分组的期间订单汇总。
//!
//! 分组只读首次生效快照（人员、组织与祖先路径），永不用现任负责人回填。

use std::collections::{BTreeMap, BTreeSet};

use persistence_core::{Executor, Transactional};
use rust_decimal::Decimal;

use super::QualityAccess;
use super::dto::{
    HistoryQualityQuery, HistoryQualityRow, HistoryQualityView, QualityPeriod, QualityRows, QualityScope,
    QualityTotals,
};
use super::query::{BASIS_LABEL, PERIOD_BASIS};
use super::source::{HistorySnapshot, QualityOrder, version};
use crate::Result;

/// 同一快照产生的历史视图与请求问询；版本校验需要原查询。
pub(super) struct HistorySnapshotOutput {
    pub view: HistoryQualityView,
    pub query: HistoryQualityQuery,
}

impl super::CustomerQualityReadModel {
    /// 在调用方事务内完成历史授权、冻结筛选与事实装载。
    pub(super) async fn snapshot_history(
        &self,
        query: &HistoryQualityQuery,
        actor: &application_core::AuditActor,
        bounds: super::query::PeriodBounds,
        export: bool,
        executor: &mut dyn Executor,
    ) -> Result<(HistorySnapshotOutput, String)> {
        let access = QualityAccess::new(self.db.clone(), self.rbac.clone(), self.customer_scope.clone());
        let (sales_context, sales_scope) = access.resolve_history(actor, executor).await?;
        if sales_scope.is_empty() {
            let mut view = assemble_history(query, Vec::new(), totals_for(&[]));
            view.empty_reason = Some("no_scope".into());
            apply_context(&mut view, &sales_context);
            let expected = sales_context.scope_version.clone();
            return Ok((HistorySnapshotOutput { view, query: query.clone() }, expected));
        }
        let order_customer_ids = query.customer_id.clone().map(|id| vec![id]);
        let filter = super::source::quality_order_filter(&bounds, order_customer_ids, &sales_scope);
        let sources = HistorySnapshot::load(&self.db, filter, executor).await?;
        let expected = version(&sales_context.scope_version, &sources.orders);
        super::ensure_version(query.scope_version.as_deref(), &expected)?;
        let mut view = project_history(query, sources);
        apply_context(&mut view, &sales_context);
        if !export {
            view.rows.items = page_items(view.rows.items, query);
        }
        Ok((HistorySnapshotOutput { view, query: query.clone() }, expected))
    }

    /// 在新的读取事务内重验销售范围与业务责任集合，防止返回旧宽范围文件。
    pub(super) async fn recheck_history(
        &self,
        query: &HistoryQualityQuery,
        actor: &application_core::AuditActor,
        expected: &str,
    ) -> Result<()> {
        let this = self.clone();
        let query = query.clone();
        let actor = actor.clone();
        let current = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                let this = this.clone();
                let query = query.clone();
                let actor = actor.clone();
                Box::pin(async move {
                    let access =
                        QualityAccess::new(this.db.clone(), this.rbac.clone(), this.customer_scope.clone());
                    let (sales_context, sales_scope) = access.resolve_history(&actor, executor).await?;
                    let bounds = query.validate()?;
                    let order_customer_ids = query.customer_id.clone().map(|id| vec![id]);
                    let filter =
                        super::source::quality_order_filter(&bounds, order_customer_ids, &sales_scope);
                    let sources = HistorySnapshot::load(&this.db, filter, executor).await?;
                    Ok::<_, crate::Error>(version(&sales_context.scope_version, &sources.orders))
                })
            })
            .await?;
        super::ensure_version(Some(expected), &current)
    }
}

/// 在冻结归属条件下汇总报表，目录由独立端点提供。
fn project_history(query: &HistoryQualityQuery, sources: HistorySnapshot) -> HistoryQualityView {
    let has_period_orders = !sources.orders.is_empty();
    let matched: Vec<&QualityOrder> =
        sources.orders.iter().filter(|o| matches_history(o, query, &sources)).collect();
    let rows = group_history_rows(&matched, &query.dimension, &sources);
    let totals = totals_for(&rows);
    let mut view = assemble_history(query, rows, totals);
    // 候选由调用方填入，此处仅决定空态。
    if view.rows.total == 0 {
        view.empty_reason = Some(if has_period_orders { "filtered_empty" } else { "no_data" }.into());
    }
    view.filter_summary = history_filter_summary(query);
    view
}

/// 同字段 OR、不同字段 AND；组织匹配冻结祖先路径，移动后不漂移。
fn matches_history(order: &QualityOrder, query: &HistoryQualityQuery, sources: &HistorySnapshot) -> bool {
    if !matches_group(order, query.attribution_group.as_deref()) {
        return false;
    }
    if let Some(ids) = &query.attribution_user_ids
        && order
            .attribution
            .as_ref()
            .map(|a| &a.attribution_user_id)
            .is_none_or(|id| !ids.as_slice().contains(id))
    {
        return false;
    }
    if let Some(ids) = &query.attribution_org_unit_ids {
        let hit = order
            .attribution
            .as_ref()
            .is_some_and(|a| a.org_path.iter().any(|node| ids.as_slice().contains(&node.id)));
        if !hit {
            return false;
        }
    }
    if let Some(q) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let lower = q.to_lowercase();
        let customer_name = sources.customer_names.get(&order.customer_id).map(String::as_str).unwrap_or("");
        if !order.order_no.to_lowercase().contains(&lower)
            && !customer_name.to_lowercase().contains(&lower)
            && order.customer_id != q
        {
            return false;
        }
    }
    true
}

/// 分组下钻匹配冻结的直接归属，不能用祖先路径条件扩大原分组；空身份仅匹配未知归属。
fn matches_group(order: &QualityOrder, group: Option<&str>) -> bool {
    let Some(group) = group else {
        return true;
    };
    let Some((dimension, id)) = group.split_once(':') else {
        return false;
    };
    let actual = match dimension {
        "attribution_user" => order.attribution.as_ref().map(|a| a.attribution_user_id.as_str()),
        "attribution_org" => order.attribution.as_ref().map(|a| a.attribution_org_unit_id.as_str()),
        _ => return false,
    };
    actual.unwrap_or_default() == id
}

/// 历史分组：冻结人员、冻结组织二者互斥；未知归属单列，不用现任回填。
fn group_history_rows(
    orders: &[&QualityOrder],
    dimension: &str,
    sources: &HistorySnapshot,
) -> Vec<HistoryQualityRow> {
    let mut groups: BTreeMap<String, Vec<&QualityOrder>> = BTreeMap::new();
    for order in orders {
        let key = match dimension {
            "attribution_user" => {
                order.attribution.as_ref().map(|a| a.attribution_user_id.clone()).unwrap_or_default()
            },
            _ => order.attribution.as_ref().map(|a| a.attribution_org_unit_id.clone()).unwrap_or_default(),
        };
        groups.entry(key).or_default().push(order);
    }
    groups
        .into_iter()
        .map(|(key, values)| {
            let (_, gross, unpriced) = aggregate_orders(&values, &sources.revision_gross);
            let (label, user_id, user_name, org_id, org_name, path) =
                group_identity(dimension, &key, &values);
            HistoryQualityRow::new(format!("{dimension}:{key}"), dimension.into(), money(gross))
                .with_group(Some(key.clone()), Some(label))
                .with_attribution(user_id, user_name, org_id, org_name, path)
                .with_counts(Some(values.len()), unpriced)
        })
        .collect()
}

/// 历史分组身份：展示名、人员身份、组织身份与祖先路径各自独立成项。
type HistoryIdentity =
    (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<Vec<String>>);

/// 分组身份只用快照名称；同一身份存在不同历史名称时稳定列出。
fn group_identity(dimension: &str, key: &str, values: &[&QualityOrder]) -> HistoryIdentity {
    if key.is_empty() {
        return ("未知归属".into(), None, None, None, None, None);
    }
    if dimension == "attribution_user" {
        let names = values
            .iter()
            .filter_map(|o| o.attribution.as_ref().map(|a| a.attribution_user_name.as_str()))
            .collect::<BTreeSet<_>>();
        let label = names.iter().cloned().collect::<Vec<_>>().join("／");
        let org = values.iter().filter_map(|o| o.attribution.as_ref()).next();
        return (
            if label.is_empty() { key.into() } else { format!("{label} · {key}") },
            Some(key.into()),
            Some(label),
            org.map(|a| a.attribution_org_unit_id.clone()),
            org.map(|a| a.attribution_org_unit_name.clone()),
            None,
        );
    }
    let names = values
        .iter()
        .filter_map(|o| o.attribution.as_ref().map(|a| a.attribution_org_unit_name.as_str()))
        .collect::<BTreeSet<_>>();
    let label = names.iter().cloned().collect::<Vec<_>>().join("／");
    let path = values
        .iter()
        .filter_map(|o| o.attribution.as_ref())
        .find(|a| a.attribution_org_unit_id == key)
        .map(|a| a.org_path.iter().map(|n| n.id.clone()).collect::<Vec<_>>());
    (
        if label.is_empty() { key.into() } else { format!("{label} · {key}") },
        None,
        None,
        Some(key.into()),
        Some(label),
        path,
    )
}

/// 分组汇总只累计一次；缺版本不写作零。
fn aggregate_orders(
    orders: &[&QualityOrder],
    gross: &BTreeMap<String, rust_decimal::Decimal>,
) -> (usize, rust_decimal::Decimal, usize) {
    let mut total = rust_decimal::Decimal::ZERO;
    let mut unpriced = 0;
    for order in orders {
        match order.current_revision_id.as_ref().and_then(|id| gross.get(id)) {
            Some(amount) => {
                total = total.checked_add(*amount).unwrap_or(rust_decimal::Decimal::MAX);
            },
            None => unpriced += 1,
        }
    }
    (orders.len(), total, unpriced)
}

/// 全量汇总不依赖当前分页；缺版本订单不计入总额但保留计数。
fn totals_for(rows: &[HistoryQualityRow]) -> QualityTotals {
    let mut orders = 0;
    let mut gross = Decimal::ZERO;
    let mut unpriced = 0;
    for row in rows {
        orders += row.order_count.unwrap_or(0);
        unpriced += row.unpriced_count;
        if let Ok(amount) = row.gross_total.parse::<Decimal>() {
            gross = gross.checked_add(amount).unwrap_or(Decimal::MAX);
        }
    }
    QualityTotals {
        object_count: rows.len(),
        order_count: orders,
        gross_total: money(gross),
        unpriced_count: unpriced,
    }
}

/// 金额保留分，服务端已完成舍入。
fn money(value: Decimal) -> String {
    format!("{value:.2}")
}

/// 排序分页最后执行；导出保留全部匹配行。
fn page_items(mut items: Vec<HistoryQualityRow>, query: &HistoryQualityQuery) -> Vec<HistoryQualityRow> {
    items.sort_by(|a, b| compare_history(a, b, &query.sort));
    items.into_iter().skip((query.page - 1) * query.page_size).take(query.page_size).collect()
}

/// 页内顺序与导出一致，同值按稳定键排序。
fn compare_history(a: &HistoryQualityRow, b: &HistoryQualityRow, sort: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (field, direction) = sort.split_once(':').unwrap_or(("orderCount", "desc"));
    let ordering = match field {
        "label" => a.label.cmp(&b.label),
        "grossTotal" => numeric(&a.gross_total).cmp(&numeric(&b.gross_total)),
        _ => a.order_count.cmp(&b.order_count),
    };
    let ordering = if direction == "desc" { ordering.reverse() } else { ordering };
    if ordering == Ordering::Equal {
        return a.row_id.cmp(&b.row_id);
    }
    ordering
}

/// 字符串金额排序使用 Decimal，禁止按字典序排列负数和大金额。
fn numeric(value: &str) -> Decimal {
    value.parse::<Decimal>().unwrap_or(Decimal::ZERO)
}

/// 页面主合同只装配各独立投影，金额与字段来源不在 HTTP 层重算。
fn assemble_history(
    query: &HistoryQualityQuery,
    rows: Vec<HistoryQualityRow>,
    totals: QualityTotals,
) -> HistoryQualityView {
    let total = rows.len();
    HistoryQualityView {
        empty_reason: None,
        scope_summary: "历史负责订单".into(),
        as_of: String::new(),
        policy_version: 0,
        organization_version: 0,
        scope_version: String::new(),
        scope: QualityScope::new("authorized".into(), "历史负责订单".into(), String::new()),
        period: QualityPeriod {
            from: query.from.clone(),
            to: query.to.clone(),
            basis: PERIOD_BASIS.into(),
            basis_label: BASIS_LABEL.into(),
            timezone: "Asia/Shanghai".into(),
        },
        ownership_basis: "sales_order_first_effective_attribution".into(),
        totals,
        rows: QualityRows { dimension: query.dimension.clone(), items: rows, total },
        filter_summary: history_filter_summary(query),
        can_export: true,
    }
}

/// 导出必须保留所有已应用筛选，内部身份只作为精确筛选追溯值。
fn history_filter_summary(query: &HistoryQualityQuery) -> String {
    let mut parts =
        vec![format!("{} 至 {}", query.from, query.to), BASIS_LABEL.into(), "口径：历史负责订单".into()];
    if let Some(ids) = &query.attribution_user_ids {
        parts.push(format!("历史归属销售：{}", ids.as_slice().join("、")));
    }
    if let Some(ids) = &query.attribution_org_unit_ids {
        parts.push(format!("历史组织路径：{}", ids.as_slice().join("、")));
    }
    if let Some(group) = &query.attribution_group {
        parts.push(format!("历史分组下钻：{group}"));
    }
    if let Some(id) = &query.customer_id {
        parts.push(format!("客户：{id}"));
    }
    if let Some(q) = &query.q {
        parts.push(format!("搜索：{q}"));
    }
    parts.join("；")
}

/// 授权时点与版本随响应返回；不暴露完整角色证明或人员集合。
fn apply_context(
    view: &mut HistoryQualityView,
    sales: &erp_identity::service::access_control::resolve::AuthorizedDataScope,
) {
    view.as_of = sales.as_of.as_utc().to_rfc3339();
    view.policy_version = sales.policy_version;
    view.organization_version = sales.organizations.version;
}
