//! 统一待办事项简报。
//!
//! 队列只展示只读业务内容，不承载确认/审批表单。采购二次确认提供客户、金额
//! 与前几行明细；采购财务审核通过 `extra_sections` 补充供应商、税额和付款条件。

use chrono::{Datelike, FixedOffset};
use entities::common::time::{BusinessDate, Instant};
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
    /// 可跳转关联单据的稳定身份；仅作路由键，不上屏。
    pub object_id: Option<String>,
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

/// 向简报追加非空键值段。关联单据请用 [`push_document_section`]。
///
/// # 参数
/// * `sections` - 已组装的键值段
/// * `label` - 标签
/// * `value` - 待写入的值
/// * `numeric` - 是否按数字对齐
///
/// # 返回
/// 无。空值不上屏。本函数不写入跳转身份。
///
/// # 错误
/// 无。
pub(crate) fn push_section(
    sections: &mut Vec<BriefSection>,
    label: &str,
    value: Option<&str>,
    numeric: bool,
) {
    let Some(value) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    sections.push(BriefSection {
        label: label.to_string(),
        value: value.to_string(),
        numeric,
        object_id: None,
    });
}

/// 向简报追加可预览或跳转的关联单据段。
///
/// # 参数
/// * `sections` - 已组装的键值段
/// * `label` - 标签
/// * `value` - 面向用户的单号
/// * `object_id` - 关联单据稳定身份；空白不上屏为链接
///
/// # 返回
/// 无。空值单号不上屏。
///
/// # 错误
/// 无。
pub(crate) fn push_document_section(
    sections: &mut Vec<BriefSection>,
    label: &str,
    value: Option<&str>,
    object_id: Option<&str>,
) {
    let Some(value) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    let object_id = object_id
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    sections.push(BriefSection {
        label: label.to_string(),
        value: value.to_string(),
        numeric: false,
        object_id,
    });
}

/// 去掉首尾空白后保留非空文本。
///
/// # 参数
/// * `value` - 原始文本
///
/// # 返回
/// 空白返回 `None`。
///
/// # 错误
/// 无。
pub(crate) fn non_empty(value: &str) -> Option<String> {
    let text = value.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// 把非空片段拼成列表一行摘要。
///
/// # 参数
/// * `parts` - 已按展示顺序排好的片段
///
/// # 返回
/// 返回用间隔符连接的摘要；全空时返回空串。
///
/// # 错误
/// 无。
pub(crate) fn join_list_summary(parts: impl IntoIterator<Item = Option<String>>) -> String {
    parts
        .into_iter()
        .flatten()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
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

/// 业务时区：东八区。工作台简报日期按此转换，不直接上 UTC。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回固定东八区偏移。
///
/// # 错误
/// 无。偏移常量合法。
fn shanghai_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("东八区偏移必须合法")
}

/// 把时刻格式化为队列交期文案。
///
/// # 参数
/// * `due` - UTC 时刻
///
/// # 返回
/// 返回东八区日历的 `8/20 交`。
///
/// # 错误
/// 无。
pub(crate) fn format_instant_due_label(due: Instant) -> String {
    let local = due.as_utc().with_timezone(&shanghai_offset());
    format!("{}/{} 交", local.month(), local.day())
}

/// 把时刻格式化为业务日期。
///
/// # 参数
/// * `at` - UTC 时刻
///
/// # 返回
/// 返回东八区 `YYYY-MM-DD`。
///
/// # 错误
/// 无。
pub(crate) fn format_instant_date(at: Instant) -> String {
    let local = at.as_utc().with_timezone(&shanghai_offset());
    format!("{}-{:02}-{:02}", local.year(), local.month(), local.day())
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
    fn instant_labels_use_shanghai_calendar() {
        let noon_utc = Instant::from_unix_secs(1_787_457_600);
        assert_eq!(format_instant_date(noon_utc), "2026-08-23");
        assert_eq!(format_instant_due_label(noon_utc), "8/23 交");
        assert_eq!(
            join_list_summary([Some("甲".into()), None, Some("¥1".into())]),
            "甲 · ¥1"
        );
        assert_eq!(non_empty("  ").as_deref(), None);
        assert_eq!(non_empty(" 客户 ").as_deref(), Some("客户"));
    }

    #[test]
    fn document_section_keeps_routing_id_off_screen() {
        let mut sections = Vec::new();
        push_document_section(&mut sections, "来源销售单", Some("SO-1"), Some("so-1"));
        assert_eq!(sections[0].label, "来源销售单");
        assert_eq!(sections[0].value, "SO-1");
        assert!(!sections[0].numeric);
        assert_eq!(sections[0].object_id.as_deref(), Some("so-1"));
    }

    #[test]
    fn document_section_skips_blank_number_and_blank_id() {
        let mut skipped = Vec::new();
        push_document_section(&mut skipped, "来源销售单", Some("  "), Some("so-1"));
        assert!(skipped.is_empty());

        let mut unlinked = Vec::new();
        push_document_section(&mut unlinked, "来源销售单", Some("SO-1"), Some("  "));
        assert_eq!(unlinked[0].value, "SO-1");
        assert_eq!(unlinked[0].object_id, None);
    }
}
