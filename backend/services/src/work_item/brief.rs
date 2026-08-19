//! 统一待办事项简报。
//!
//! 队列只展示只读业务内容，不承载确认/审批表单。采购二次确认提供客户、金额
//! 与前几行明细；采购财务审核通过 `extra_sections` 补充供应商、税额和付款条件。

use entities::common::time::BusinessDate;
use entities::money::Quantity;

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
}
