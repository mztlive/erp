//! 同步导出全部匹配分组；防止电子表格公式注入并附带本次查询口径。
use super::dto::{ProfitLossExport, ProfitLossRow, ProfitLossView};
use crate::support::csv_record as record;

/// 直接序列化服务端全量结果，不使用客户端金额或当前页数据。
///
/// # 参数
/// * `view` - 服务端全量盈亏视图。
///
/// # 返回
/// 返回 CSV 内容、文件名、行数和生成时点。
///
/// # 错误
/// 不返回错误。
pub(super) fn export(view: ProfitLossView) -> ProfitLossExport {
    let mut lines = metadata(&view);
    lines.push(header());
    lines.extend(view.rows.items.iter().map(row));
    ProfitLossExport {
        csv_content: lines.join("\r\n"),
        file_name: format!("实际盈亏-非卡券不含税-{}_{}.csv", view.period.from, view.period.to),
        row_count: view.rows.total,
        generated_at: view.freshness.projected_at,
    }
}
/// 导出文件记录生成时的口径和授权范围。
fn metadata(view: &ProfitLossView) -> Vec<String> {
    vec![
        record(&["实际经营盈亏", "非卡券", "不含税"]),
        record(&["开始日期", &view.period.from, "结束日期", &view.period.to]),
        record(&["期间口径", &view.period.basis_label, "生成时点", &view.freshness.projected_at]),
        record(&["数据范围", &view.scope.label, "计算规则", &view.formula_version]),
        record(&["筛选", &view.filter_summary]),
        record(&["统计说明", &view.excluded_note]),
    ]
}
/// 列名与行数据顺序固定。
fn header() -> String {
    record(&[
        "对象",
        "客户",
        "首次生效归属销售",
        "首次生效归属组织",
        "不含税收入",
        "实际采购成本",
        "实际履约费用",
        "成本冲减",
        "实际盈亏",
        "利润率",
        "成本覆盖",
        "缺口原因",
    ])
}
/// 每个分组行使用服务端已校验金额，缺利润保留空值。
fn row(row: &ProfitLossRow) -> String {
    let t = &row.totals;
    let blockers = row.coverage_blockers.iter().map(|b| b.message.as_str()).collect::<Vec<_>>().join("；");
    let coverage = match row.coverage_state.as_str() {
        "COVERED" => "完整",
        "PARTIAL" => "部分",
        _ => "未覆盖",
    };
    record(&[
        &row.identity_label,
        row.customer_label.as_deref().unwrap_or(""),
        row.attribution_user_name.as_deref().unwrap_or(""),
        row.attribution_org_unit_name.as_deref().unwrap_or(""),
        &t.net_sales_revenue,
        &t.actual_procurement_cost_net,
        &t.actual_fulfillment_cost_net,
        &t.reductions_net,
        t.actual_profit_loss_net.as_deref().unwrap_or(""),
        t.margin_rate.as_deref().unwrap_or(""),
        coverage,
        &blockers,
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::actual_profit_loss::dto::{CoverageBlocker, Totals};

    #[test]
    fn row_preserves_column_order_negative_amounts_and_missing_profit() {
        let mut value =
            ProfitLossRow::new("row".into(), "sales_order".into(), "@订单".into(), "PARTIAL".into());
        value.customer_label = Some("客户\"甲".into());
        value.attribution_user_name = Some("冻结销售".into());
        value.totals = Totals {
            net_sales_revenue: "100.00".into(),
            actual_procurement_cost_net: "120.00".into(),
            actual_fulfillment_cost_net: "0.00".into(),
            reductions_net: "0.00".into(),
            ..Totals::default()
        };
        value.coverage_blockers =
            vec![CoverageBlocker { code: "missing".into(), message: "缺少成本".into() }];
        assert_eq!(
            row(&value),
            "\"'@订单\",\"客户\"\"甲\",\"冻结销售\",\"\",\"100.00\",\"120.00\",\"0.00\",\"0.00\",\"\",\"\",\"部分\",\"缺少成本\""
        );
        value.totals.actual_profit_loss_net = Some("-20.00".into());
        value.totals.margin_rate = Some("-20.00%".into());
        value.coverage_state = "COVERED".into();
        value.coverage_blockers.clear();
        assert_eq!(
            row(&value),
            "\"'@订单\",\"客户\"\"甲\",\"冻结销售\",\"\",\"100.00\",\"120.00\",\"0.00\",\"0.00\",\"-20.00\",\"'-20.00%\",\"完整\",\"\""
        );
    }

    #[test]
    fn csv_blocks_formula_and_preserves_negative_amount() {
        assert_eq!(record(&[" =HYPERLINK(\"x\")"]), "\"' =HYPERLINK(\"\"x\"\")\"");
        assert_eq!(record(&["-12.50"]), "\"-12.50\"");
        assert_eq!(record(&["a,b\nc"]), "\"a,b\nc\"");
    }
}
