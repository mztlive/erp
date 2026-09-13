//! 内存正式事实回归：覆盖、分组、筛选、分页及导出，不连接 MongoDB。
use super::{calculation, dto::ProfitLossQuery, projection, source::Sources};
use erp_core::{
    common::time::Instant,
    money::{Amount, Rate},
};
use erp_finance::entity::cost::*;
use erp_sales::{entity::sales_order::*, repository::sales_order::profit_loss::ProfitLossOrder};
use std::str::FromStr;

/// 固定上海九月生效的单行销售及实际成本事实。
fn sources() -> Sources {
    let now = instant("2026-09-01T00:00:00Z");
    let revision = SalesOrderRevision::new(
        "revision-1".to_owned().into(),
        SalesOrderRevisionData {
            sales_order_id: "order-1".to_owned().into(),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            previous_revision_id: None,
            content_hash: "hash".into(),
            customer_revision_id: None,
            contract_revision_id: None,
            snapshot: HeaderSnapshotData {
                customer_name: "测试客户".into(),
                payment_term_code: "CASH".into(),
                payment_term_name: "现结".into(),
                invoice_type: "普通发票".into(),
                tax_point: "13%".into(),
                ..Default::default()
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amount("113"),
            net_amount: amount("100"),
            tax_amount: amount("13"),
            effective_at: now,
            recorded_at: now,
        },
    )
    .unwrap();
    let line = SalesOrderRevisionLine::new(
        "revision-line-1".to_owned().into(),
        SalesOrderRevisionLineData {
            sales_order_revision_id: "revision-1".to_owned().into(),
            sales_order_line_id: "line-1".to_owned().into(),
            line_no: 1,
            line_type: LineType::GoodsService,
            gross_amount: amount("113"),
            net_amount: amount("100"),
            tax_amount: amount("13"),
            sales_tax_rate: Rate::from_str("0.13").unwrap(),
            item_name_snapshot: "礼盒".into(),
            spec_snapshot: None,
            unit_snapshot: Some("盒".into()),
        },
    )
    .unwrap();
    let mut result = Sources {
        orders: vec![ProfitLossOrder {
            id: "order-1".into(),
            order_no: "SO-001".into(),
            version: 1,
            customer_id: "customer-1".into(),
            current_revision_id: Some("revision-1".into()),
            effective_at: Some(now),
            fulfillment_progress: FulfillmentProgress::Completed,
            attribution: None,
        }],
        revisions: vec![revision],
        lines: vec![line],
        goods: vec![],
        allocations: vec![],
        entries: vec![],
    };
    cost(
        &mut result,
        "purchase",
        CostStage::Actual,
        CostType::Product,
        "60",
    );
    cost(
        &mut result,
        "delivery",
        CostStage::Actual,
        CostType::Delivery,
        "10",
    );
    cost(
        &mut result,
        "reduction",
        CostStage::Reduction,
        CostType::Product,
        "5",
    );
    cost(
        &mut result,
        "expected",
        CostStage::Expected,
        CostType::Product,
        "90",
    );
    cost(
        &mut result,
        "confirmed",
        CostStage::Confirmed,
        CostType::Product,
        "80",
    );
    result
}
/// 正式金额解析。
fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}
/// 成本分配与事实一起构造，测试不绕过领域金额校验。
fn cost(source: &mut Sources, id: &str, stage: CostStage, kind: CostType, net: &str) {
    let entry = CostEntry::new(
        id.to_owned().into(),
        CostEntryData {
            cost_type: kind,
            cost_stage: stage,
            cost_scope: CostScope::NonVoucherFulfillment,
            cost_basis: None,
            supplier_id: None,
            gross_amount: amount(net),
            net_amount: amount(net),
            tax_amount: amount("0"),
            tax_inclusion: true,
            input_tax_rate: Rate::from_str("0").unwrap(),
            occurred_at: instant("2026-09-02T00:00:00Z"),
            source_fact_type: "manual".into(),
            source_document_id: id.into(),
            source_line_id: "line-1".into(),
            source_version: "1".into(),
            adjusts_cost_entry_id: None,
            evidence_attachment_id: None,
        },
    )
    .unwrap();
    let allocation = CostAllocation::new(
        format!("allocation-{id}").into(),
        CostAllocationData {
            cost_entry_id: id.to_owned().into(),
            sales_order_id: Some("order-1".to_owned().into()),
            sales_order_line_id: Some("line-1".to_owned().into()),
            allocated_gross_amount: amount(net),
            allocated_net_amount: amount(net),
            rounding_residual_flag: false,
        },
    )
    .unwrap();
    source.entries.push(entry);
    source.allocations.push(allocation);
}
/// 所有测试使用同一显式期间。
fn query() -> ProfitLossQuery {
    ProfitLossQuery {
        from: "2026-09-01".into(),
        to: "2026-09-30".into(),
        period_basis: "sales_order_effective_date".into(),
        coverage: "all".into(),
        dimension: "sales_order".into(),
        sort: "actualProfitLossNet:asc".into(),
        page: 1,
        page_size: 20,
        ..Default::default()
    }
}
/// 成本发生之后的固定查询时点。
fn as_of() -> i64 {
    instant("2026-09-11T00:00:00Z").as_utc().timestamp()
}
#[test]
fn uses_net_revenue_and_actual_allocations_with_one_reduction() {
    let result = calculation::calculate(&sources(), as_of(), true).unwrap();
    let row = &result[0].row;
    assert_eq!(row.totals.actual_profit_loss_net.as_deref(), Some("35.00"));
    assert_eq!(row.totals.margin_rate.as_deref(), Some("35.00%"));
    assert_eq!(row.coverage_state, "COVERED");
    assert_eq!(row.cost_entry_ids.len(), 3);
    assert_eq!(result[0].costs.confirmed_procurement.to_string(), "80");
}
#[test]
fn missing_cost_unfinished_delivery_and_future_facts_never_create_profit() {
    let mut source = sources();
    source.entries.retain(|e| e.base.id != "purchase");
    let result = calculation::calculate(&source, as_of(), false).unwrap();
    assert!(result[0].row.totals.actual_profit_loss_net.is_none());
    assert!(result[0].row.allowed_drilldowns.is_empty());
    assert!(result[0]
        .row
        .coverage_blockers
        .iter()
        .any(|b| b.code == "COST_FACT_MISSING"));
    let mut source = sources();
    source.orders[0].fulfillment_progress = FulfillmentProgress::PartiallyFulfilled;
    assert!(calculation::calculate(&source, as_of(), true).unwrap()[0]
        .row
        .totals
        .actual_profit_loss_net
        .is_none());
    assert!(
        calculation::calculate(&sources(), as_of() - 20 * 86400, true).unwrap()[0]
            .row
            .totals
            .actual_profit_loss_net
            .is_none()
    );
}
#[test]
fn voucher_scope_and_excess_reduction_do_not_inflate_profit() {
    let mut source = sources();
    source.entries[0].cost_scope = CostScope::CardDirectFulfillment;
    assert!(calculation::calculate(&source, as_of(), true).unwrap()[0]
        .row
        .totals
        .actual_profit_loss_net
        .is_none());
    let mut source = sources();
    cost(
        &mut source,
        "too-much",
        CostStage::Reduction,
        CostType::Product,
        "200",
    );
    assert!(calculation::calculate(&source, as_of(), true).unwrap()[0]
        .row
        .coverage_blockers
        .iter()
        .any(|b| b.code == "EXCESS_REDUCTION"));
}
#[test]
fn current_revision_must_match_order_and_line_revenue() {
    let mut source = sources();
    source.revisions[0].sales_order_id = "other-order".to_owned().into();
    assert!(calculation::calculate(&source, as_of(), true).is_err());
    let mut source = sources();
    source.lines.clear();
    assert!(calculation::calculate(&source, as_of(), true).is_err());
}
#[test]
fn paging_does_not_change_totals_and_export_contains_all_rows() {
    let template = calculation::calculate(&sources(), as_of(), true)
        .unwrap()
        .remove(0);
    let orders: Vec<_> = (0..25)
        .map(|n| {
            let mut o = template.clone();
            o.row.row_id = format!("order-{n:02}");
            o
        })
        .collect();
    let mut q = query();
    q.page = 2;
    let page = projection::project(orders.clone(), &q, "2026-09-11", false, "测试范围").unwrap();
    assert_eq!(page.rows.items.len(), 5);
    assert_eq!(page.rows.total, 25);
    assert_eq!(page.totals.actual_profit_loss_net.as_deref(), Some("875.00"));
    let full = projection::project(orders, &q, "2026-09-11", true, "测试范围").unwrap();
    let exported = super::export::export(full);
    assert_eq!(exported.row_count, 25);
    assert_eq!(exported.csv_content.matches("SO-001").count(), 25);
}
#[test]
fn cost_filter_keeps_whole_order_cost_and_grouping_never_duplicates_revenue() {
    let mut orders = calculation::calculate(&sources(), as_of(), true).unwrap();
    orders[0].row.benefit_scenarios = vec!["生日".into(), "节日".into()];
    let mut q = query();
    q.cost_types = Some("delivery".into());
    q.dimension = "scenario".into();
    let view = projection::project(orders, &q, "2026-09-11", false, "测试范围").unwrap();
    assert_eq!(view.rows.total, 1);
    assert_eq!(view.totals.net_sales_revenue, "100.00");
    assert_eq!(view.totals.actual_profit_loss_net.as_deref(), Some("35.00"));
    assert!(view.rows.items[0].object_id.is_none());
}

#[test]
fn historical_groups_use_frozen_ids_and_preserve_unknown_contribution() {
    let mut source = sources();
    source.orders[0].attribution = Some(SalesAttribution {
        attribution_user_id: "sales-old".into(),
        attribution_user_name: "张三".into(),
        attribution_org_unit_id: "org-old".into(),
        attribution_org_unit_name: "销售一部".into(),
        attributed_at: source.orders[0].effective_at.unwrap(),
        attribution_version: 1,
        organization_version: 1,
        org_path: vec![AttributionOrgNode {
            id: "org-old".into(),
            name: "销售一部".into(),
        }],
    });
    let mut orders = calculation::calculate(&source, as_of(), false).unwrap();
    let mut same_name = orders[0].clone();
    same_name.row.attribution_user_id = Some("sales-other".into());
    same_name.row.attribution_org_unit_id = Some("org-other".into());
    orders.push(same_name);
    orders.extend(calculation::calculate(&sources(), as_of(), false).unwrap());
    for dimension in ["attribution_user", "attribution_org"] {
        let mut q = query();
        q.dimension = dimension.into();
        let view = projection::project(orders.clone(), &q, "2026-09-11", true, "测试范围").unwrap();
        assert_eq!(view.rows.total, 3);
        assert_eq!(view.totals.net_sales_revenue, "300.00");
        assert_eq!(view.totals.actual_profit_loss_net.as_deref(), Some("105.00"));
        assert!(view.rows.items.iter().any(|r| r.identity_label == "未知归属"));
        assert!(view.rows.items.iter().all(|r| r.customer_id.is_none()));
        assert_eq!(super::export::export(view).row_count, 3);
    }
}

#[test]
fn attribution_filters_intersect_before_totals_and_keep_full_scope_candidates() {
    let mut orders = calculation::calculate(&sources(), as_of(), false).unwrap();
    orders[0].row.attribution_user_id = Some("sales-a".into());
    orders[0].row.attribution_user_name = Some("同名销售".into());
    orders[0].attribution_path = vec![AttributionOrgNode {
        id: "old-parent".into(),
        name: "原部门".into(),
    }];
    let mut other = orders[0].clone();
    other.row.attribution_user_id = Some("sales-b".into());
    other.row.row_id = "other".into();
    orders.push(other);
    let mut q = query();
    q.attribution_user_ids = Some(serde_json::from_value(serde_json::json!("sales-a")).unwrap());
    q.attribution_org_unit_ids = Some(serde_json::from_value(serde_json::json!("old-parent")).unwrap());
    let view = projection::project(orders.clone(), &q, "2026-09-11", true, "测试范围").unwrap();
    assert_eq!(view.rows.total, 1);
    assert_eq!(view.totals.net_sales_revenue, "100.00");
    assert_eq!(view.totals.actual_profit_loss_net.as_deref(), Some("35.00"));
    assert_eq!(view.attribution_user_options.len(), 2);
    assert_ne!(
        view.attribution_user_options[0].label,
        view.attribution_user_options[1].label
    );
    q.attribution_org_unit_ids = Some(serde_json::from_value(serde_json::json!("current-parent")).unwrap());
    let empty = projection::project(orders, &q, "2026-09-11", true, "测试范围").unwrap();
    assert_eq!(empty.rows.total, 0);
    assert_eq!(empty.totals.net_sales_revenue, "0.00");
}

#[test]
fn stale_scope_version_requires_refresh() {
    assert!(super::ensure_version(None, "new").is_ok());
    assert!(super::ensure_version(Some("new"), "new").is_ok());
    assert!(matches!(
        super::ensure_version(Some("old"), "new"),
        Err(crate::Error::ConflictError(_))
    ));
}

#[test]
fn group_drilldown_preserves_exact_partition_including_unknown_attribution() {
    let mut orders = calculation::calculate(&sources(), as_of(), true).unwrap();
    orders[0].row.attribution_org_unit_id = Some("parent".into());
    let mut child = orders[0].clone();
    child.row.row_id = "child-order".into();
    child.row.attribution_org_unit_id = Some("child".into());
    let mut unknown = child.clone();
    unknown.row.row_id = "unknown-order".into();
    unknown.row.attribution_org_unit_id = None;
    orders.extend([child, unknown]);
    let mut q = query();
    for group in [
        "attribution_org:parent",
        "attribution_org:child",
        "attribution_org:",
    ] {
        q.attribution_group = Some(group.into());
        assert!(q.validate().is_ok());
        let view = projection::project(orders.clone(), &q, "2026-09-11", true, "范围").unwrap();
        assert_eq!(view.rows.total, 1);
        assert_eq!(view.totals.net_sales_revenue, "100.00");
        assert!(view.empty_reason.is_none());
    }
    q.attribution_group = Some("attribution_org:unavailable".into());
    let empty = projection::project(orders, &q, "2026-09-11", true, "范围").unwrap();
    assert_eq!(empty.empty_reason.as_deref(), Some("filtered_empty"));
    let empty = projection::project(Vec::new(), &q, "2026-09-11", true, "范围").unwrap();
    assert_eq!(empty.empty_reason.as_deref(), Some("no_data"));
    for group in ["created_by:user", "attribution_org:a,b", "attribution_org:$where"] {
        q.attribution_group = Some(group.into());
        assert!(q.validate().is_err());
    }
}

#[test]
fn responsibility_revision_or_visible_order_changes_invalidate_query_version() {
    let mut source = sources();
    let initial = super::source::version("scope", &source.orders);
    source.orders[0].version += 1;
    assert_ne!(super::source::version("scope", &source.orders), initial);
    assert_ne!(super::source::version("scope", &[]), initial);
    assert_ne!(super::source::version("revoked", &source.orders), initial);
}
#[test]
fn partial_coverage_keeps_missing_revenue_in_denominator() {
    let mut orders = calculation::calculate(&sources(), as_of(), true).unwrap();
    let mut missing = orders[0].clone();
    missing.row.row_id = "missing".into();
    missing.row.coverage_state = "PARTIAL".into();
    missing.row.totals.actual_profit_loss_net = None;
    orders.push(missing);
    let mut q = query();
    q.coverage = "covered".into();
    let view = projection::project(orders, &q, "2026-09-11", false, "测试范围").unwrap();
    assert_eq!(view.coverage.coverage_rate, "50.00%");
    assert_eq!(view.rows.total, 1);
    assert_eq!(view.totals.net_sales_revenue, "100.00");
}

/// 固定时间解析，仓储时间值使用秒级时间戳。
fn instant(value: &str) -> Instant {
    Instant::from_unix_secs(chrono::DateTime::parse_from_rfc3339(value).unwrap().timestamp())
}
#[test]
fn response_shape_preserves_decimal_strings_and_missing_profit() {
    let mut orders = calculation::calculate(&sources(), as_of(), true).unwrap();
    let mut incomplete = orders[0].clone();
    incomplete.row.row_id = "order-missing".into();
    incomplete.row.object_id = Some("order-missing".into());
    incomplete.row.identity_label = "SO-002".into();
    incomplete
        .row
        .coverage_blockers
        .push(super::dto::CoverageBlocker {
            code: "SUPPLY_COST_MISSING".into(),
            message: "部分销售明细缺少实际供货成本".into(),
        });
    incomplete.finish().unwrap();
    orders.push(incomplete);
    let view = projection::project(
        orders,
        &query(),
        "2026-09-11T10:00:00+00:00",
        false,
        "测试财务范围",
    )
    .unwrap();
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["totals"]["actualProfitLossNet"], "35.00");
    assert!(json["rows"]["items"][1].get("actualProfitLossNet").is_none());
    assert_eq!(json["coverage"]["coverageRate"], "50.00%");
    if std::env::var_os("ERP_PROFIT_LOSS_QA").is_some() {
        println!("PROFIT_LOSS_QA={json}");
    }
}

#[test]
fn literal_keyword_filters_do_not_interpret_regular_expressions() {
    let mut orders = calculation::calculate(&sources(), as_of(), true).unwrap();
    orders[0].row.identity_label = "SO.[1]".into();
    let mut q = query();
    q.q = Some(".[1]".into());
    let view = projection::project(orders.clone(), &q, "2026-09-11", false, "测试范围").unwrap();
    assert_eq!(view.rows.total, 1);
    q.q = Some(".*".into());
    let empty = projection::project(orders, &q, "2026-09-11", false, "测试范围").unwrap();
    assert_eq!(empty.rows.total, 0);
    assert!(empty.totals.actual_profit_loss_net.is_none());
}
