//! 双口径同步导出；防止电子表格公式注入并附带本次查询口径。
use super::dto::{CurrentQualityView, HistoryQualityView, QualityExport};
use crate::support::csv_record as record;

/// 直接序列化服务端全量结果，不使用客户端金额或当前页数据。
///
/// # 参数
/// * `view` - 已装载的现任口径全量视图
///
/// # 返回
/// 返回 CSV 正文、文件名、`view.rows.total` 行数和生成时点。
///
/// # 错误
/// 不返回错误。
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
        generated_at: view.as_of,
    }
}

/// 历史口径导出冻结归属列；现任负责人永不出现在历史文件中。
///
/// # 参数
/// * `view` - 已装载的历史口径全量视图
///
/// # 返回
/// 返回 CSV 正文、文件名、`view.rows.total` 行数和生成时点。
///
/// # 错误
/// 不返回错误。
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
        generated_at: view.as_of,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customer_quality::dto::{
        CurrentQualityRow, HistoryQualityRow, QualityPeriod, QualityRows, QualityScope, QualityTotals,
    };

    fn period() -> QualityPeriod {
        QualityPeriod {
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
            basis: "effective_date".into(),
            basis_label: "生效日期".into(),
            timezone: "Asia/Shanghai".into(),
        }
    }

    #[test]
    fn current_export_preserves_rows_columns_fallbacks_and_metadata() {
        let rows = QualityRows {
            dimension: "customer".into(),
            items: vec![
                CurrentQualityRow::new("c-1".into(), "customer".into(), "-12.50".into())
                    .with_customer(Some("c-1".into()), Some("C-1".into()), Some("=客户".into()))
                    .with_ownership(Some("u-1".into()), None, Some("org-1".into()), None)
                    .with_counts(2, 1),
                CurrentQualityRow::new("group-1".into(), "owner_user".into(), "0.00".into()).with_group(
                    None,
                    Some("分组\"名".into()),
                    None,
                ),
            ],
            total: 2,
        };
        let output = current(CurrentQualityView {
            empty_reason: None,
            scope_summary: String::new(),
            as_of: "2026-10-01".into(),
            scope_version: "v3".into(),
            scope: QualityScope::new("all".into(), "授权范围".into(), "v3".into()),
            period: period(),
            ownership_basis: "current_owner".into(),
            totals: QualityTotals::default(),
            rows,
            filter_summary: "筛选说明".into(),
            can_export: true,
        });
        let lines: Vec<_> = output.csv_content.split("\r\n").collect();
        assert_eq!(lines.len(), 9);
        assert_eq!(
            lines[6],
            "\"客户\",\"客户编号\",\"现任负责人\",\"现任组织\",\"订单数\",\"含税总额\",\"缺版本数\",\"首次生效\",\"最近生效\""
        );
        assert_eq!(lines[7], "\"'=客户\",\"C-1\",\"u-1\",\"org-1\",\"2\",\"-12.50\",\"1\",\"\",\"\"");
        assert_eq!(lines[8], "\"分组\"\"名\",\"\",\"\",\"\",\"0\",\"0.00\",\"0\",\"\",\"\"");

        assert_eq!(output.file_name, "客户经营质量-当前负责-2026-09-01-2026-09-30.csv");
        assert_eq!(output.generated_at, "2026-10-01");
        assert_eq!(output.row_count, 2);
    }

    #[test]
    fn history_export_keeps_frozen_attribution_and_empty_headers() {
        for empty in [false, true] {
            let output = history(HistoryQualityView {
                empty_reason: None,
                scope_summary: String::new(),
                as_of: "2026-10-01".into(),
                scope_version: "v4".into(),
                scope: QualityScope::new("all".into(), "授权范围".into(), "v4".into()),
                period: period(),
                ownership_basis: "frozen_attribution".into(),
                totals: QualityTotals::default(),
                rows: QualityRows {
                    dimension: "attribution_user".into(),
                    items: if empty {
                        vec![]
                    } else {
                        vec![
                            HistoryQualityRow::new(
                                "history".into(),
                                "attribution_user".into(),
                                "10.00".into(),
                            )
                            .with_group(None, Some("历史分组".into()))
                            .with_attribution(
                                Some("frozen-user".into()),
                                None,
                                Some("frozen-org".into()),
                                None,
                                None,
                            ),
                        ]
                    },
                    total: usize::from(!empty),
                },
                filter_summary: "历史筛选".into(),
                can_export: true,
            });
            let lines: Vec<_> = output.csv_content.split("\r\n").collect();
            assert_eq!(lines.len(), if empty { 7 } else { 8 });
            assert_eq!(
                lines[6],
                "\"历史分组\",\"首次生效归属销售\",\"首次生效归属组织\",\"订单数\",\"含税总额\",\"缺版本数\""
            );
            if !empty {
                assert_eq!(lines[7], "\"历史分组\",\"frozen-user\",\"frozen-org\",\"0\",\"10.00\",\"0\"");
            }

            assert_eq!(output.file_name, "客户经营质量-历史贡献-2026-09-01-2026-09-30.csv");
            assert_eq!(output.generated_at, "2026-10-01");
            assert_eq!(output.row_count, usize::from(!empty));
        }
    }

    #[test]
    fn csv_blocks_formula_and_preserves_negative_amount() {
        assert_eq!(record(&[" =HYPERLINK(\"x\")"]), "\"' =HYPERLINK(\"\"x\"\")\"");
        assert_eq!(record(&["-12.50"]), "\"-12.50\"");
        assert_eq!(record(&["a,b\nc"]), "\"a,b\nc\"");
    }
}
