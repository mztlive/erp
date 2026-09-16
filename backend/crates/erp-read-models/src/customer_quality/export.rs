//! 双口径同步导出；防止电子表格公式注入并附带本次查询口径。
use super::dto::{CurrentQualityView, HistoryQualityView, QualityExport};

/// 直接序列化服务端全量结果，不使用客户端金额或当前页数据。
pub(super) fn current(view: CurrentQualityView) -> QualityExport {
    let mut lines = vec![
        record(&["客户经营质量", "当前负责口径", "含税"]),
        record(&["开始日期", &view.period.from, "结束日期", &view.period.to]),
        record(&["期间口径", &view.period.basis_label, "生成时点", &view.as_of]),
        record(&["数据范围", &view.scope.label, "归属口径", &view.ownership_basis]),
        record(&["筛选", &view.filter_summary]),
        record(&["范围版本", &view.scope_version]),
    ];
    lines.push(record(&[
        "客户",
        "客户编号",
        "现任负责人",
        "现任组织",
        "订单数",
        "含税总额",
        "缺版本数",
        "首次生效",
        "最近生效",
    ]));
    lines.extend(view.rows.items.iter().map(|row| {
        record(&[
            row.customer_name.as_deref().unwrap_or(row.label.as_deref().unwrap_or("")),
            row.customer_no.as_deref().unwrap_or(""),
            row.owner_user_name.as_deref().or(row.owner_user_id.as_deref()).unwrap_or(""),
            row.owner_org_unit_name.as_deref().or(row.owner_org_unit_id.as_deref()).unwrap_or(""),
            &row.order_count.to_string(),
            &row.gross_total,
            &row.unpriced_count.to_string(),
            row.first_effective_at.as_deref().unwrap_or(""),
            row.latest_effective_at.as_deref().unwrap_or(""),
        ])
    }));
    QualityExport {
        csv_content: lines.join("\r\n"),
        file_name: format!("客户经营质量-当前负责-{}-{}.csv", view.period.from, view.period.to),
        row_count: view.rows.total,
        generated_at: view.as_of.clone(),
    }
}

/// 历史口径导出冻结归属列；现任负责人永不出现在历史文件中。
pub(super) fn history(view: HistoryQualityView) -> QualityExport {
    let mut lines = vec![
        record(&["客户经营质量", "历史贡献口径", "含税"]),
        record(&["开始日期", &view.period.from, "结束日期", &view.period.to]),
        record(&["期间口径", &view.period.basis_label, "生成时点", &view.as_of]),
        record(&["数据范围", &view.scope.label, "归属口径", &view.ownership_basis]),
        record(&["筛选", &view.filter_summary]),
        record(&["范围版本", &view.scope_version]),
    ];
    lines.push(record(&[
        "历史分组",
        "首次生效归属销售",
        "首次生效归属组织",
        "订单数",
        "含税总额",
        "缺版本数",
    ]));
    lines.extend(view.rows.items.iter().map(|row| {
        record(&[
            row.label.as_deref().unwrap_or(""),
            row.attribution_user_name.as_deref().or(row.attribution_user_id.as_deref()).unwrap_or(""),
            row.attribution_org_unit_name.as_deref().or(row.attribution_org_unit_id.as_deref()).unwrap_or(""),
            &row.order_count.unwrap_or(0).to_string(),
            &row.gross_total,
            &row.unpriced_count.to_string(),
        ])
    }));
    QualityExport {
        csv_content: lines.join("\r\n"),
        file_name: format!("客户经营质量-历史贡献-{}-{}.csv", view.period.from, view.period.to),
        row_count: view.rows.total,
        generated_at: view.as_of.clone(),
    }
}

/// CSV 单元格转义；文本控制字符或公式起始符添加单引号，合法负数保留数值。
fn record(values: &[&str]) -> String {
    values.iter().map(|v| cell(v)).collect::<Vec<_>>().join(",")
}

/// 引号、换行和危险前缀不能改变表格结构或执行公式。
fn cell(value: &str) -> String {
    let trimmed = value.trim_start();
    let numeric = value.parse::<rust_decimal::Decimal>().is_ok();
    let dangerous =
        !numeric && (trimmed.starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n']));
    let prefix = if dangerous { "'" } else { "" };
    format!("\"{prefix}{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_blocks_formula_and_preserves_negative_amount() {
        assert_eq!(cell(" =HYPERLINK(\"x\")"), "\"' =HYPERLINK(\"\"x\"\")\"");
        assert_eq!(cell("-12.50"), "\"-12.50\"");
        assert_eq!(cell("a,b\nc"), "\"a,b\nc\"");
    }

    #[test]
    fn current_and_history_exports_use_separate_files_and_bases() {
        let current = serde_json::to_string(&"current").unwrap();
        let history = serde_json::to_string(&"history").unwrap();
        assert_ne!(current, history);
        let source = include_str!("export.rs");
        assert!(source.contains("当前负责口径"));
        assert!(source.contains("历史贡献口径"));
    }
}
