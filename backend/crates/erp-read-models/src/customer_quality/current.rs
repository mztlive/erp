//! 当前负责口径：现任主责客户及其期间订单汇总。
//!
//! 分组只用现任归属（客户当前主责与主责所属组织），永不读取冻结归属快照。

use std::collections::{BTreeMap, BTreeSet};

use erp_customer::ports::CustomerDataScopePort;
use persistence_core::{Executor, Transactional};
use rust_decimal::Decimal;

use super::QualityAccess;
use super::access::no_current_scope;
use super::dto::{
    CurrentQualityQuery, CurrentQualityRow, CurrentQualityView, QualityPeriod, QualityRows, QualityScope,
    QualityTotals,
};
use super::query::{BASIS_LABEL, PERIOD_BASIS};
use super::source::{CustomerFact, QUALITY_ORDER_LIMIT, QualityOrder, effective_label};
use crate::Result;

/// 同一快照产生的当前视图与请求问询；版本校验需要原查询。
pub(super) struct CurrentSnapshotOutput {
    pub view: CurrentQualityView,
    pub query: CurrentQualityQuery,
}

impl super::CustomerQualityReadModel {
    /// 在调用方事务内完成现任授权、业务筛选与事实装载。
    pub(super) async fn snapshot_current(
        &self,
        query: &CurrentQualityQuery,
        actor: &application_core::AuditActor,
        bounds: super::query::PeriodBounds,
        export: bool,
        executor: &mut dyn Executor,
    ) -> Result<(CurrentSnapshotOutput, String)> {
        let access = QualityAccess::new(self.db.clone(), self.rbac.clone(), self.customer_scope.clone());
        let (customer_context, customer_scope, sales_context, sales_scope) =
            access.resolve_current(actor, executor).await?;
        if no_current_scope(&customer_context, &customer_scope) && sales_scope.is_empty() {
            let mut view = assemble_current(query, Vec::new(), totals_for(&[]));
            view.empty_reason = Some("no_scope".into());
            apply_context(&mut view, &customer_context, &sales_context);
            let expected = format!("{}:{}", customer_context.scope_version, sales_context.scope_version);
            return Ok((CurrentSnapshotOutput { view, query: query.clone() }, expected));
        }
        let expanded = expand_requested_orgs(self.customer_scope.as_ref(), query, executor).await?;
        let authorized = customer_scope.authorized_customer_ids.clone();
        if let Some(ids) = &authorized
            && ids.len() > QUALITY_ORDER_LIMIT
        {
            return Err(crate::Error::ValidationError("客户查询超过上限，请收窄组织或负责人条件".into()));
        }
        // 订单先按销售范围与显式客户装载，客户授权在内存中求交，避免公司范围全表扫描。
        let order_customer_ids = match (&authorized, &query.customer_id) {
            (Some(ids), None) => Some(ids.clone()),
            (Some(ids), Some(requested)) => {
                Some(if ids.contains(requested) { vec![requested.clone()] } else { Vec::new() })
            },
            (None, requested) => requested.clone().map(|id| vec![id]),
        };
        let order_filter = super::source::quality_order_filter(&bounds, order_customer_ids, &sales_scope);
        let raw_orders = super::source::CurrentSources::load_orders(&self.db, order_filter, executor).await?;
        // 客户集合为订单客户与授权集合的交集；显式客户无订单时仍保留沉默行。
        let mut customer_ids = raw_orders.iter().map(|o| o.customer_id.clone()).collect::<BTreeSet<_>>();
        if let Some(ids) = &authorized {
            let allowed = ids.iter().cloned().collect::<BTreeSet<_>>();
            customer_ids = customer_ids.intersection(&allowed).cloned().collect();
        }
        if let Some(requested) = &query.customer_id
            && is_customer_allowed(&authorized, requested)
        {
            customer_ids.insert(requested.clone());
        }
        let mut sources = super::source::CurrentSources::load_customers(
            &self.db,
            customer_ids.into_iter().collect(),
            &raw_orders,
            executor,
        )
        .await?;
        apply_request_filters(&mut sources, query, expanded.as_ref());
        let mut view = project_current(query, sources);
        let no_scope =
            !customer_context.has_scope_rules() && view.rows.total == 0 && view.totals.order_count == 0;
        if no_scope {
            view.empty_reason = Some("no_scope".into());
        }
        apply_context(&mut view, &customer_context, &sales_context);
        let expected = combined_version(&customer_context.scope_version, &sales_context.scope_version, &view);
        super::ensure_version(query.scope_version.as_deref(), &expected)?;
        if !export {
            view.rows.items = page_items(view.rows.items, query);
        }
        Ok((CurrentSnapshotOutput { view, query: query.clone() }, expected))
    }

    /// 在新的读取事务内重验账号、组织、协作及业务责任集合，防止返回旧宽范围文件。
    pub(super) async fn recheck_current(
        &self,
        query: &CurrentQualityQuery,
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
                    let (customer_context, customer_scope, sales_context, sales_scope) =
                        access.resolve_current(&actor, executor).await?;
                    let expanded =
                        expand_requested_orgs(this.customer_scope.as_ref(), &query, executor).await?;
                    let authorized = customer_scope.authorized_customer_ids.clone();
                    let order_customer_ids = match (&authorized, &query.customer_id) {
                        (Some(ids), None) => Some(ids.clone()),
                        (Some(ids), Some(requested)) => {
                            Some(if ids.contains(requested) { vec![requested.clone()] } else { Vec::new() })
                        },
                        (None, requested) => requested.clone().map(|id| vec![id]),
                    };
                    let filter = super::source::quality_order_filter(
                        &query.validate()?,
                        order_customer_ids,
                        &sales_scope,
                    );
                    let raw_orders =
                        super::source::CurrentSources::load_orders(&this.db, filter, executor).await?;
                    let mut customer_ids =
                        raw_orders.iter().map(|o| o.customer_id.clone()).collect::<BTreeSet<_>>();
                    if let Some(ids) = &authorized {
                        let allowed = ids.iter().cloned().collect::<BTreeSet<_>>();
                        customer_ids = customer_ids.intersection(&allowed).cloned().collect();
                    }
                    if let Some(requested) = &query.customer_id
                        && is_customer_allowed(&authorized, requested)
                    {
                        customer_ids.insert(requested.clone());
                    }
                    let mut sources = super::source::CurrentSources::load_customers(
                        &this.db,
                        customer_ids.into_iter().collect(),
                        &raw_orders,
                        executor,
                    )
                    .await?;
                    apply_request_filters(&mut sources, &query, expanded.as_ref());
                    let mut view = project_current(&query, sources);
                    view.rows.items = page_items(view.rows.items, &query);
                    apply_context(&mut view, &customer_context, &sales_context);
                    Ok::<_, crate::Error>(combined_version(
                        &customer_context.scope_version,
                        &sales_context.scope_version,
                        &view,
                    ))
                })
            })
            .await?;
        super::ensure_version(Some(expected), &current)
    }
}

/// 显式客户必须落在授权集合内；公司范围允许任意显式客户。
fn is_customer_allowed(authorized: &Option<Vec<String>>, requested: &str) -> bool {
    authorized.as_ref().is_none_or(|ids| ids.contains(&requested.to_string()))
}

/// 请求组织展开为启用节点；未知组织拒绝，不得忽略后查询全部。
async fn expand_requested_orgs(
    port: &dyn CustomerDataScopePort,
    query: &CurrentQualityQuery,
    executor: &mut dyn Executor,
) -> Result<Option<BTreeSet<String>>> {
    let Some(org_ids) = &query.org_unit_ids else {
        if query.include_descendants == Some(true) {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        return Ok(None);
    };
    if query.include_descendants == Some(true) && org_ids.as_slice().is_empty() {
        return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
    }
    let expanded = port
        .expand_org_units(org_ids.as_slice(), query.include_descendants.unwrap_or(false), executor)
        .await
        .map_err(crate::Error::from)?;
    Ok(Some(expanded))
}

/// 现任负责人、组织、分组与关键词在同一快照内求交；只看现任归属。
fn apply_request_filters(
    sources: &mut super::source::CurrentSnapshot,
    query: &CurrentQualityQuery,
    expanded: Option<&BTreeSet<String>>,
) {
    if let Some(owners) = &query.owner_user_ids {
        sources
            .customers
            .retain(|c| c.owner_user_id.as_ref().is_some_and(|id| owners.as_slice().contains(id)));
    }
    if let Some(orgs) = expanded {
        sources.customers.retain(|c| c.owner_org_unit_id.as_ref().is_some_and(|id| orgs.contains(id)));
    }
    if let Some(group) = &query.owner_group {
        sources.customers.retain(|c| matches_owner_group(c, Some(group)));
    }
    if let Some(requested) = &query.customer_id {
        sources.customers.retain(|c| &c.id == requested);
    }
    if let Some(q) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let lower = q.to_lowercase();
        let order_hit = sources
            .orders
            .iter()
            .filter(|o| o.order_no.to_lowercase().contains(&lower))
            .map(|o| o.customer_id.clone())
            .collect::<BTreeSet<_>>();
        sources.customers.retain(|c| {
            c.customer_no.to_lowercase().contains(&lower)
                || c.name.to_lowercase().contains(&lower)
                || order_hit.contains(&c.id)
        });
    }
    let kept = sources.customers.iter().map(|c| c.id.clone()).collect::<BTreeSet<_>>();
    sources.orders.retain(|o| kept.contains(&o.customer_id));
    if let Some(q) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let _ = q;
    }
}

/// 现任分组下钻匹配当前主责或主责所属组织；空后缀仅匹配未知组织。
fn matches_owner_group(customer: &CustomerFact, group: Option<&str>) -> bool {
    let Some(group) = group else {
        return true;
    };
    let Some((dimension, id)) = group.split_once(':') else {
        return false;
    };
    match dimension {
        "user" => customer.owner_user_id.as_deref().unwrap_or_default() == id,
        "org" => customer.owner_org_unit_id.as_deref().unwrap_or_default() == id,
        _ => false,
    }
}

/// 同一快照内投影汇总、分组与排序；排序分页最后执行。
fn project_current(
    query: &CurrentQualityQuery,
    sources: super::source::CurrentSnapshot,
) -> CurrentQualityView {
    let has_period_orders = !sources.orders.is_empty() || !sources.customers.is_empty();
    let orders_by_customer = group_orders_by_customer(&sources.orders);
    let mut rows = Vec::new();
    for customer in &sources.customers {
        let orders = orders_by_customer.get(&customer.id).map(Vec::as_slice).unwrap_or(&[]);
        let (order_count, gross_total, unpriced, first, latest) =
            summarize_orders(orders, &sources.revision_gross);
        rows.push(
            CurrentQualityRow::new(
                format!("customer:{}", customer.id),
                "customer".into(),
                money(gross_total),
            )
            .with_customer(
                Some(customer.id.clone()),
                Some(customer.customer_no.clone()),
                Some(customer.name.clone()),
            )
            .with_ownership(
                customer.owner_user_id.clone(),
                customer.owner_user_id.as_ref().and_then(|id| sources.account_names.get(id).cloned()),
                customer.owner_org_unit_id.clone(),
                customer.owner_org_unit_id.as_ref().and_then(|id| sources.org_names.get(id).cloned()),
            )
            .with_counts(order_count, unpriced)
            .with_effective_range(
                first.and_then(|at| effective_label(Some(at))),
                latest.and_then(|at| effective_label(Some(at))),
            ),
        );
    }
    let grouped = group_current_rows(rows, &query.dimension, &sources);
    let totals = totals_for(&grouped);
    let mut view = assemble_current(query, grouped, totals);
    if view.rows.total == 0 {
        view.empty_reason = Some(if has_period_orders { "filtered_empty" } else { "no_data" }.into());
    }
    view
}

/// 按客户聚合订单引用；同一订单只归属其当前客户一次。
fn group_orders_by_customer(orders: &[QualityOrder]) -> BTreeMap<String, Vec<&QualityOrder>> {
    let mut map: BTreeMap<String, Vec<&QualityOrder>> = BTreeMap::new();
    for order in orders {
        map.entry(order.customer_id.clone()).or_default().push(order);
    }
    map
}

/// 汇总订单数、含税总额与缺版本数；缺版本不写作零。
fn summarize_orders(
    orders: &[&QualityOrder],
    gross: &BTreeMap<String, Decimal>,
) -> (usize, Decimal, usize, Option<i64>, Option<i64>) {
    let mut total = Decimal::ZERO;
    let mut unpriced = 0;
    let mut first = None;
    let mut latest = None;
    for order in orders {
        match order.current_revision_id.as_ref().and_then(|id| gross.get(id)) {
            Some(amount) => {
                total = total.checked_add(*amount).unwrap_or(Decimal::MAX);
            },
            None => unpriced += 1,
        }
        if let Some(at) = order.effective_at {
            first = Some(first.map_or(at, |v: i64| v.min(at)));
            latest = Some(latest.map_or(at, |v: i64| v.max(at)));
        }
    }
    (orders.len(), total, unpriced, first, latest)
}

/// 现任分组：客户行、负责人分组、组织分组三者互斥，永不混排。
fn group_current_rows(
    rows: Vec<CurrentQualityRow>,
    dimension: &str,
    sources: &super::source::CurrentSnapshot,
) -> Vec<CurrentQualityRow> {
    if dimension == "customer" {
        return rows;
    }
    let mut groups: BTreeMap<String, Vec<CurrentQualityRow>> = BTreeMap::new();
    for row in rows {
        let key = match dimension {
            "owner_user" => row.owner_user_id.clone().unwrap_or_default(),
            _ => row.owner_org_unit_id.clone().unwrap_or_default(),
        };
        groups.entry(key).or_default().push(row);
    }
    groups
        .into_iter()
        .map(|(key, values)| {
            let (order_count, gross, unpriced, customer_count) = aggregate_group(&values);
            let (label, user_name, org_name) = group_labels(dimension, &key, &values, sources);
            CurrentQualityRow::new(
                format!("{dimension}:{key}"),
                if dimension == "owner_user" { "owner_user".into() } else { "owner_org".into() },
                money(gross),
            )
            .with_group(Some(key.clone()), Some(label), Some(customer_count))
            .with_ownership(
                (dimension == "owner_user" && !key.is_empty()).then(|| key.clone()),
                user_name,
                (dimension == "owner_org" && !key.is_empty()).then(|| key.clone()),
                org_name,
            )
            .with_counts(order_count, unpriced)
            .with_effective_range(
                values.iter().filter_map(|r| r.first_effective_at.clone()).min(),
                values.iter().filter_map(|r| r.latest_effective_at.clone()).max(),
            )
        })
        .collect()
}

/// 分组汇总只累计一次；缺版本数按行累加。
fn aggregate_group(rows: &[CurrentQualityRow]) -> (usize, Decimal, usize, usize) {
    let mut orders = 0;
    let mut gross = Decimal::ZERO;
    let mut unpriced = 0;
    for row in rows {
        orders += row.order_count;
        unpriced += row.unpriced_count;
        if let Ok(amount) = row.gross_total.parse::<Decimal>() {
            gross = gross.checked_add(amount).unwrap_or(Decimal::MAX);
        }
    }
    (orders, gross, unpriced, rows.len())
}

/// 分组标签只用现任名称；未知分组单列，不用其他分组回填。
fn group_labels(
    dimension: &str,
    key: &str,
    values: &[CurrentQualityRow],
    sources: &super::source::CurrentSnapshot,
) -> (String, Option<String>, Option<String>) {
    if key.is_empty() {
        return ("未知归属".into(), None, None);
    }
    if dimension == "owner_user" {
        let name = sources.account_names.get(key).cloned().unwrap_or_else(|| {
            values.iter().filter_map(|r| r.owner_user_name.clone()).next().unwrap_or_default()
        });
        let label = if name.is_empty() { key.into() } else { format!("{name} · {key}") };
        return (label, Some(name), None);
    }
    let name = sources.org_names.get(key).cloned().unwrap_or_else(|| {
        values.iter().filter_map(|r| r.owner_org_unit_name.clone()).next().unwrap_or_default()
    });
    let label = if name.is_empty() { key.into() } else { format!("{name} · {key}") };
    (label, None, Some(name))
}

/// 全量汇总不依赖当前分页；缺版本订单不计入总额但保留计数。
fn totals_for(rows: &[CurrentQualityRow]) -> QualityTotals {
    let mut orders = 0;
    let mut gross = Decimal::ZERO;
    let mut unpriced = 0;
    for row in rows {
        orders += row.order_count;
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
fn page_items(mut items: Vec<CurrentQualityRow>, query: &CurrentQualityQuery) -> Vec<CurrentQualityRow> {
    items.sort_by(|a, b| compare_current(a, b, &query.sort));
    items.into_iter().skip((query.page - 1) * query.page_size).take(query.page_size).collect()
}

/// 页内顺序与导出一致，同值按稳定键排序。
fn compare_current(a: &CurrentQualityRow, b: &CurrentQualityRow, sort: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (field, direction) = sort.split_once(':').unwrap_or(("orderCount", "desc"));
    let ordering = match field {
        "customerNo" => a.customer_no.cmp(&b.customer_no),
        "label" => a.label.cmp(&b.label),
        "customerCount" => a.customer_count.cmp(&b.customer_count),
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
fn assemble_current(
    query: &CurrentQualityQuery,
    rows: Vec<CurrentQualityRow>,
    totals: QualityTotals,
) -> CurrentQualityView {
    let total = rows.len();
    CurrentQualityView {
        empty_reason: None,
        scope_summary: "当前负责客户".into(),
        as_of: String::new(),
        policy_version: 0,
        organization_version: 0,
        scope_version: String::new(),
        scope: QualityScope::new("authorized".into(), "当前负责客户".into(), String::new()),
        period: QualityPeriod {
            from: query.from.clone(),
            to: query.to.clone(),
            basis: PERIOD_BASIS.into(),
            basis_label: BASIS_LABEL.into(),
            timezone: "Asia/Shanghai".into(),
        },
        ownership_basis: "customer_current_assignment".into(),
        totals,
        rows: QualityRows { dimension: query.dimension.clone(), items: rows, total },
        filter_summary: current_filter_summary(query),
        can_export: true,
    }
}

/// 导出必须保留所有已应用筛选，内部身份只作为精确筛选追溯值。
fn current_filter_summary(query: &CurrentQualityQuery) -> String {
    let mut parts =
        vec![format!("{} 至 {}", query.from, query.to), BASIS_LABEL.into(), "口径：当前负责客户".into()];
    if let Some(ids) = &query.owner_user_ids {
        parts.push(format!("现任负责人：{}", ids.as_slice().join("、")));
    }
    if let Some(ids) = &query.org_unit_ids {
        parts.push(format!("现任组织：{}", ids.as_slice().join("、")));
    }
    if let Some(id) = &query.customer_id {
        parts.push(format!("客户：{id}"));
    }
    if let Some(group) = &query.owner_group {
        parts.push(format!("现任分组下钻：{group}"));
    }
    if let Some(q) = &query.q {
        parts.push(format!("搜索：{q}"));
    }
    parts.join("；")
}

/// 授权时点与版本随响应返回；不暴露完整角色证明或人员集合。
fn apply_context(
    view: &mut CurrentQualityView,
    customer: &erp_customer::CustomerResolvedScope,
    sales: &erp_identity::service::access_control::resolve::AuthorizedDataScope,
) {
    view.as_of = customer.as_of.as_utc().to_rfc3339();
    view.policy_version = customer.policy_version;
    view.organization_version = customer.organization_version.max(sales.organizations.version);
}

/// 组合版本覆盖客户与销售双重授权及业务责任版本；不返回身份集合。
fn combined_version(customer_version: &str, sales_version: &str, view: &CurrentQualityView) -> String {
    use std::hash::{Hash, Hasher};
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    view.rows.items.iter().map(|r| &r.row_id).collect::<Vec<_>>().hash(&mut fingerprint);
    view.totals.order_count.hash(&mut fingerprint);
    format!("{customer_version}:{sales_version}:{:x}", fingerprint.finish())
}
