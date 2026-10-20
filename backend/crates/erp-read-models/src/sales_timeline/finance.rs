//! 分配行的纯展示投影，金额不得由整笔资金替代。
use super::TimelineCategory;
use super::sources::Sources;

#[allow(clippy::too_many_arguments)]
pub(super) fn allocation(
    sources: &mut Sources,
    document_id: &str,
    id: &str,
    time: u64,
    amount: String,
    action: &str,
    label: &str,
    category: TimelineCategory,
) {
    let Some(document) = sources.documents.get(document_id) else {
        return;
    };
    let verb = if action == "reverse" { "冲减本单分配" } else { "分配至本单" };
    let mut event =
        document.event(format!("allocation:{id}"), time, format!("{label}{verb}"), "未记录操作人".into());
    event.category = category;
    if let Some(document) = sources.documents.get_mut(document_id) {
        document.category_override = Some(category);
    }
    event.amount = Some(amount);
    event.amount_label = Some(if action == "reverse" { "本次冲减金额" } else { "本次归属本单金额" }.into());
    sources.facts.push(event);
}
