//! 统一待办事项简报。
//!
//! 队列只展示只读业务内容，不承载确认/审批表单。采购二次确认先打通：
//! 客户、金额、前几行品名/数量/交期、提交来源。

use chrono::{Datelike, FixedOffset, TimeZone};
use entities::common::time::Instant;
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
    push_section(&mut sections, "含税金额", source.amount_label.as_deref(), true);
    push_section(
        &mut sections,
        "提交来源",
        submission_origin_label(reason_code),
        false,
    );
    push_section(&mut sections, "提交人", source.submitter_name.as_deref(), false);
    AssembledBrief {
        sections,
        lines: source.lines.clone(),
        more_count: source.more_count,
        list_summary: source.list_summary.clone(),
    }
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

fn line_title(item_name: &str, spec: Option<&str>) -> String {
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

fn format_quantity(quantity: &Quantity, unit: Option<&str>) -> String {
    let number = quantity.to_decimal().normalize().to_string();
    match unit.map(str::trim).filter(|text| !text.is_empty()) {
        Some(unit) => format!("{number} {unit}"),
        None => format!("×{number}"),
    }
}

fn format_due_label(due_at: Instant) -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("东八区偏移合法");
    let local = offset.from_utc_datetime(&due_at.as_utc().naive_utc());
    format!("{}/{} 交", local.month(), local.day())
}

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
}
