//! 统一待办事项简报。
//!
//! 队列只展示只读业务内容，不承载确认/审批表单。采购二次确认提供客户、金额
//! 与前几行明细；采购财务审核通过 `extra_sections` 补充供应商、税额和付款条件。

use chrono::{Datelike, FixedOffset, TimeZone};
use entities::common::time::{BusinessDate, Instant};
use entities::money::{Amount, Quantity};

use super::presentation::format_yuan;

/// 简报最多展开的销售明细行数。
pub(crate) const BRIEF_LINE_LIMIT: usize = 3;

/// 对象事实上携带的只读事项内容。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ObjectBriefSource {
    pub customer: Option<String>,
    pub amount_label: Option<String>,
    pub lines: Vec<BriefLine>,
    pub more_count: u32,
    pub submitter_name: Option<String>,
    pub list_summary: String,
    pub extra_sections: Vec<BriefSection>,
}

/// 简报中的一行销售明细。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BriefLine {
    pub title: String,
    pub quantity: Option<String>,
    pub due_label: Option<String>,
}

/// 简报键值段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BriefSection {
    pub label: String,
    pub value: String,
    pub numeric: bool,
}

/// 组装后的事项简报。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AssembledBrief {
    pub sections: Vec<BriefSection>,
    pub lines: Vec<BriefLine>,
    pub more_count: u32,
    pub list_summary: String,
}

/// 按原因码翻译提交来源。
///
/// # 参数
/// * `reason_code` - 任务原因码
///
/// # 返回
/// 已知来源返回中文；未知返回 `None`。
///
/// # 错误
/// 无。
pub(crate) fn submission_origin_label(reason_code: Option<&str>) -> Option<&'static str> {
    match reason_code
        .unwrap_or("")
        .trim()
        .replace('-', "_")
        .to_ascii_lowercase()
        .as_str()
    {
        "procurement_confirmation_dispatched" => Some("初次提交"),
        "procurement_confirmation_resubmitted" => Some("驳回后重提"),
        "low_margin_approved_procurement_confirmation" => Some("低毛利通过后再确认"),
        _ => None,
    }
}

/// 把销售提交行格式化成简报行。
///
/// # 参数
/// * `item_name` - 品名快照
/// * `spec` - 规格快照
/// * `quantity` - 基础单位数量
/// * `unit` - 单位
/// * `due_at` - 客户期望交期
///
/// # 返回
/// 返回品名、数量和交期文案。
///
/// # 错误
/// 无。
#[allow(dead_code)]
pub(crate) fn brief_line_from_submission(
    item_name: &str,
    spec: Option<&str>,
    quantity: Option<&Quantity>,
    unit: Option<&str>,
    due_at: Option<Instant>,
) -> BriefLine {
    BriefLine {
        title: line_title(item_name, spec),
        quantity: quantity.map(|qty| format_quantity(qty, unit)),
        due_label: due_at.map(format_due_label),
    }
}

/// 用销售提交规模生成对象事实中的简报源。
///
/// # 参数
/// * `customer` - 客户名称
/// * `gross_amount` - 提交含税金额
/// * `lines` - 已按行号排好的简报行
/// * `submitter_name` - 已解析的提交人姓名
///
/// # 返回
/// 返回截断后的简报源和列表一行摘要。
///
/// # 错误
/// 无。
#[allow(dead_code)]
pub(crate) fn object_brief_source(
    customer: Option<String>,
    gross_amount: Option<&Amount>,
    mut lines: Vec<BriefLine>,
    submitter_name: Option<String>,
) -> ObjectBriefSource {
    let more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
    lines.truncate(BRIEF_LINE_LIMIT);
    let amount_label = gross_amount.map(format_yuan);
    let list_summary = list_summary(customer.as_deref(), amount_label.as_deref(), &lines, more_count);
    ObjectBriefSource {
        customer,
        amount_label,
        lines,
        more_count,
        submitter_name,
        list_summary,
        extra_sections: Vec::new(),
    }
}

/// 把对象简报源和任务原因码组装成可上屏简报。
///
/// # 参数
/// * `source` - 对象事实上的事项内容
/// * `reason_code` - 任务原因码，用于提交来源
///
/// # 返回
/// 返回详情键值、明细行和列表摘要。
///
/// # 错误
/// 无。
pub(crate) fn assemble_brief(source: &ObjectBriefSource, reason_code: Option<&str>) -> AssembledBrief {
    let mut sections = Vec::new();
    push_section(&mut sections, "客户", source.customer.as_deref(), false);
    sections.extend(source.extra_sections.iter().cloned());
    if !has_section(&sections, "含税金额") {
        push_section(&mut sections, "含税金额", source.amount_label.as_deref(), true);
    }
    if !has_section(&sections, "提交来源") {
        push_section(
            &mut sections,
            "提交来源",
            submission_origin_label(reason_code),
            false,
        );
    }
    if !has_section(&sections, "提交人") {
        push_section(&mut sections, "提交人", source.submitter_name.as_deref(), false);
    }
    AssembledBrief {
        sections,
        lines: source.lines.clone(),
        more_count: source.more_count,
        list_summary: source.list_summary.clone(),
    }
}

/// 判断简报是否已有同名键值，避免任务类型专属段与默认段重复上屏。
///
/// # 参数
/// * `sections` - 已组装的键值段
/// * `label` - 要查找的标签
///
/// # 返回
/// 已存在同名段时返回 `true`。
///
/// # 错误
/// 无。
fn has_section(sections: &[BriefSection], label: &str) -> bool {
    sections.iter().any(|section| section.label == label)
}

fn push_section(sections: &mut Vec<BriefSection>, label: &str, value: Option<&str>, numeric: bool) {
    let Some(value) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    sections.push(BriefSection {
        label: label.to_string(),
        value: value.to_string(),
        numeric,
    });
}

/// 用商品名称和规格拼简报行标题。
///
/// # 参数
/// * `item_name` - 品名快照
/// * `spec` - 规格快照；超长时丢弃以免撑爆队列行
///
/// # 返回
/// 返回可读标题；名称与规格都空时回退「未命名明细」。
///
/// # 错误
/// 无。
pub(crate) fn line_title(item_name: &str, spec: Option<&str>) -> String {
    let name = item_name.trim();
    let spec = spec
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.chars().count() <= 16);
    match spec {
        Some(spec) if !name.is_empty() => format!("{name} {spec}"),
        _ if !name.is_empty() => name.to_string(),
        Some(spec) => spec.to_string(),
        None => "未命名明细".to_string(),
    }
}

/// 把数量和单位格式化为简报数量文案。
///
/// # 参数
/// * `quantity` - 基础单位数量
/// * `unit` - 单位快照
///
/// # 返回
/// 有单位时返回 `20 件`；无单位时返回 `×20`。
///
/// # 错误
/// 无。
pub(crate) fn format_quantity(quantity: &Quantity, unit: Option<&str>) -> String {
    let number = quantity.to_decimal().normalize().to_string();
    match unit.map(str::trim).filter(|text| !text.is_empty()) {
        Some(unit) => format!("{number} {unit}"),
        None => format!("×{number}"),
    }
}

#[allow(dead_code)]
fn format_due_label(due_at: Instant) -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("东八区偏移合法");
    let local = offset.from_utc_datetime(&due_at.as_utc().naive_utc());
    format!("{}/{} 交", local.month(), local.day())
}

/// 把采购预计交期格式化为队列交期文案。
///
/// # 参数
/// * `due` - 业务自然日
///
/// # 返回
/// 返回 `8/20 交` 这类相对日历文案。
///
/// # 错误
/// 无。
pub(crate) fn format_business_due_label(due: BusinessDate) -> String {
    let (_, month, day) = due.ymd();
    format!("{month}/{day} 交")
}

#[allow(dead_code)]
fn list_summary(
    customer: Option<&str>,
    amount_label: Option<&str>,
    lines: &[BriefLine],
    more_count: u32,
) -> String {
    let mut parts = Vec::new();
    if let Some(first) = lines.first() {
        let mut head = first.title.clone();
        if let Some(quantity) = first.quantity.as_deref() {
            head.push(' ');
            head.push_str(quantity);
        }
        if let Some(due) = first.due_label.as_deref() {
            head.push_str(" · ");
            head.push_str(due);
        }
        parts.push(head);
    } else if let Some(customer) = customer.map(str::trim).filter(|text| !text.is_empty()) {
        parts.push(customer.to_string());
    }
    if more_count > 0 {
        parts.push(format!("另 {more_count} 行"));
    }
    if let Some(amount) = amount_label {
        parts.push(amount.to_string());
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_labels_use_business_language() {
        assert_eq!(
            submission_origin_label(Some("procurement_confirmation_dispatched")),
            Some("初次提交")
        );
        assert_eq!(
            submission_origin_label(Some("PROCUREMENT_CONFIRMATION_RESUBMITTED")),
            Some("驳回后重提")
        );
        assert_eq!(
            submission_origin_label(Some("low_margin_approved_procurement_confirmation")),
            Some("低毛利通过后再确认")
        );
        assert_eq!(submission_origin_label(Some("other")), None);
    }

    #[test]
    fn brief_keeps_first_three_lines_and_builds_list_summary() {
        let amount = "12800".parse::<Amount>().expect("测试金额必须合法");
        let qty = "20".parse::<Quantity>().expect("测试数量必须合法");
        let due = Instant::from_unix_secs(1_787_270_400);
        let lines = vec![
            brief_line_from_submission("办公椅", None, Some(&qty), None, Some(due)),
            brief_line_from_submission("书桌", Some("1.2m"), None, None, None),
            brief_line_from_submission("灯", None, None, None, None),
            brief_line_from_submission("垃圾桶", None, None, None, None),
        ];
        let source = object_brief_source(
            Some("东方企业".to_string()),
            Some(&amount),
            lines,
            Some("周航".into()),
        );
        assert_eq!(source.lines.len(), 3);
        assert_eq!(source.more_count, 1);
        assert!(source.list_summary.contains("办公椅"));
        assert!(source.list_summary.contains("另 1 行"));
        assert!(source.list_summary.contains("¥12,800"));

        let assembled = assemble_brief(&source, Some("procurement_confirmation_dispatched"));
        assert_eq!(assembled.sections[0].label, "客户");
        assert_eq!(assembled.sections[0].value, "东方企业");
        assert!(assembled
            .sections
            .iter()
            .any(|section| section.label == "提交来源" && section.value == "初次提交"));
        assert!(assembled
            .sections
            .iter()
            .any(|section| section.label == "提交人" && section.value == "周航"));
        assert_eq!(assembled.more_count, 1);
    }

    #[test]
    fn extra_sections_keep_type_specific_order_without_duplicating_defaults() {
        let source = ObjectBriefSource {
            extra_sections: vec![
                BriefSection {
                    label: "供应商".to_string(),
                    value: "华东纸业".to_string(),
                    numeric: false,
                },
                BriefSection {
                    label: "含税金额".to_string(),
                    value: "¥12,800".to_string(),
                    numeric: true,
                },
                BriefSection {
                    label: "提交来源".to_string(),
                    value: "初次提交".to_string(),
                    numeric: false,
                },
            ],
            amount_label: Some("¥1".to_string()),
            submitter_name: Some("周航".into()),
            list_summary: "华东纸业".to_string(),
            ..ObjectBriefSource::default()
        };
        let assembled = assemble_brief(&source, Some("procurement_confirmation_dispatched"));
        let labels = assembled
            .sections
            .iter()
            .map(|section| section.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["供应商", "含税金额", "提交来源", "提交人"]);
        assert_eq!(
            format_business_due_label(BusinessDate::from_ymd(2026, 8, 20).unwrap()),
            "8/20 交"
        );
    }
}
