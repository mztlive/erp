//! 变更摘要按不可变提交编号保存，历史审批不得读取当前草稿原因。
use super::*;
use crate::workbench::WorkbenchSubjectDisplay;

/// 保存每次销售变更的冻结字段和行差异。
///
/// `fact` 为待补充展示，`change` 为变更实体，`number` 为来源单号，`context` 为批量提交事实。
/// 无返回值；仅写入展示，不修改权限。
pub(super) fn sales(
    fact: &mut WorkbenchObjectFact,
    change: &SalesChangeOrder,
    number: Option<&str>,
    context: &SalesChangeBriefContext,
) {
    for submission in context
        .submissions
        .values()
        .filter(|row| row.sales_change_order_id.as_ref() == change.base.id)
    {
        let base = context
            .base_revisions
            .get(&submission.base_revision_id.to_string());
        let (lines, more) = lines(
            context.base_lines.get(&submission.base_revision_id.to_string()),
            context.target_lines.get(&submission.base.id),
        );
        let source = sales_source(change, number, base, submission, lines, more);
        let display = WorkbenchSubjectDisplay {
            counterparty_label: Some(submission.customer_snapshot.customer_name.clone()),
            impact_summary: fact.display.impact_summary.clone(),
            brief_source: Some(source),
        };
        fact.display
            .subject_briefs
            .insert(submission.submission_no.to_string(), display.clone());
        fact.display
            .subject_briefs
            .insert(submission.base.id.clone(), display);
    }
}

/// 原因和变更类型没有提交快照；只有仍冻结的当前提交可使用实体头。
fn sales_source(
    change: &SalesChangeOrder,
    number: Option<&str>,
    base: Option<&SalesOrderRevision>,
    submission: &SalesChangeSubmission,
    lines: Vec<BriefLine>,
    more: u32,
) -> ObjectBriefSource {
    let current = !change.is_draft()
        && change.current_submission_id.as_ref().map(|id| id.as_ref()) == Some(submission.base.id.as_str());
    let mut source = sales_change_brief_source(change, number, base, Some(submission), lines, more);
    if !current {
        source
            .extra_sections
            .retain(|section| section.label != "原因" && section.label != "变更类型");
        source.list_summary = join_list_summary([
            number.map(|no| format!("销售单 {no}")),
            Some(format!("第 {} 次提交", submission.submission_no)),
            source.amount_label.clone(),
        ]);
    }
    source
}

/// 保存采购变更的冻结字段；仅完整且与审批计数一致的 CS 序列可作为历史版本。
///
/// `fact` 为待补充展示，`change` 为变更实体，`number` 为来源单号，`context` 为批量提交事实。
/// 无返回值；仅写入展示，不修改权限。
pub(super) fn purchase(
    fact: &mut WorkbenchObjectFact,
    change: &PurchaseChangeOrder,
    number: Option<&str>,
    context: &PurchaseChangeBriefContext,
) {
    let aligned = purchase_versions_aligned(change, context);
    for submission in context
        .submissions
        .values()
        .filter(|row| row.purchase_change_order_id.as_ref() == change.base.id)
    {
        let base = context
            .base_revisions
            .get(&submission.base_revision_id.to_string());
        let (lines, more) = lines(
            context.base_lines.get(&submission.base_revision_id.to_string()),
            context.target_lines.get(&submission.base.id),
        );
        let source = purchase_source(change, number, base, submission, lines, more);
        let display = WorkbenchSubjectDisplay {
            counterparty_label: Some(submission.supplier_snapshot.supplier_name.clone()),
            impact_summary: fact.display.impact_summary.clone(),
            brief_source: Some(source),
        };
        let current =
            change.current_submission_id.as_ref().map(|id| id.as_ref()) == Some(submission.base.id.as_str());
        let version = if current {
            Some(change.approval_subject_version).filter(|version| *version > 0)
        } else if aligned {
            purchase_sequence(submission)
        } else {
            None
        };
        if let Some(version) = version {
            fact.display
                .subject_briefs
                .insert(version.to_string(), display.clone());
        }
        fact.display
            .subject_briefs
            .insert(submission.base.id.clone(), display);
    }
}

/// 仅接受标准正整数 CS 编号，旧格式不得推测审批版本。
fn purchase_sequence(submission: &PurchaseChangeSubmission) -> Option<u32> {
    submission
        .submission_no
        .strip_prefix("CS-")?
        .parse()
        .ok()
        .filter(|version| *version > 0)
}

/// 验证完整提交序列与当前审批计数及提交指针一致；迁移或缺号时仅显示明确绑定的当前提交。
fn purchase_versions_aligned(change: &PurchaseChangeOrder, context: &PurchaseChangeBriefContext) -> bool {
    let rows = context
        .submissions
        .values()
        .filter(|row| row.purchase_change_order_id.as_ref() == change.base.id)
        .collect::<Vec<_>>();
    let Some(current) = rows.iter().find(|row| {
        change.current_submission_id.as_ref().map(|id| id.as_ref()) == Some(row.base.id.as_str())
    }) else {
        return false;
    };
    if purchase_sequence(current) != Some(change.approval_subject_version)
        || rows.len() != change.approval_subject_version as usize
    {
        return false;
    }
    let mut versions = rows
        .iter()
        .filter_map(|row| purchase_sequence(row))
        .collect::<Vec<_>>();
    versions.sort_unstable();
    versions.len() == rows.len() && versions.iter().copied().eq(1..=change.approval_subject_version)
}

/// 不把可修改的采购变更原因冒充历史原因。
fn purchase_source(
    change: &PurchaseChangeOrder,
    number: Option<&str>,
    base: Option<&PurchaseOrderRevision>,
    submission: &PurchaseChangeSubmission,
    lines: Vec<BriefLine>,
    more: u32,
) -> ObjectBriefSource {
    let current = change.stable.status
        != erp_procurement::entity::purchase_order::PurchaseChangeOrderStatus::Draft
        && change.current_submission_id.as_ref().map(|id| id.as_ref()) == Some(submission.base.id.as_str());
    let mut source = purchase_change_brief_source(change, number, base, Some(submission), lines, more);
    if !current {
        source.extra_sections.retain(|section| section.label != "原因");
        source.list_summary = join_list_summary([
            number.map(|no| format!("采购单 {no}")),
            Some(submission.supplier_snapshot.supplier_name.clone()),
            source.amount_label.clone(),
        ]);
    }
    source
}

/// 使用同一差异规则和简报行数上限。
fn lines(base: Option<&LineStateMap>, target: Option<&LineStateMap>) -> (Vec<BriefLine>, u32) {
    let mut rows = change_diff_lines(base, target);
    let more = rows.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
    rows.truncate(BRIEF_LINE_LIMIT);
    (rows, more)
}

#[cfg(test)]
mod tests;
