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
            customer_id: "customer-1".into(),
            current_revision_id: Some("revision-1".into()),
            effective_at: Some(now),
            fulfillment_progress: FulfillmentProgress::Completed,
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
