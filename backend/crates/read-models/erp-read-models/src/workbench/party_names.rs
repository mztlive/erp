//! 为已授权的跨域审批摘要补齐提交人姓名。
//!
//! 队列页不得把「当前处理人」这种占位词上屏；姓名按账号主数据批量解析。
//! 简报里的「提交人」段由各 `*_brief.rs` 写入账号 ID（见 `sales_order_brief.rs`
//! 的 `submitted_by`），同样在这里解析；解析不到就整段摘掉——界面禁止展示账号 ID。

use std::collections::HashMap;

use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use persistence_core::Executor;

use super::dto::WorkItemSummarySection;
use super::{WorkbenchFactsReader, WorkbenchReadService};
use crate::errors::Result;

/// 简报里提交人段的标签，与 `brief.rs` 写入端保持一致。
const SUBMITTER_LABEL: &str = "提交人";

impl WorkbenchReadService {
    // 组装跨域审批人员名称时，审批跟踪复用责任队列的提交人姓名解析；无法解析的账号不展示。
    //
    // 调用参数：
    // * `items` - 已完成阅读授权的审批列表
    // * `executor` - 账号姓名读取沿用的执行器
    //
    // 原地将提交人身份替换为姓名。
    //
    // 账号批量读取失败时返回仓储错误。
    pub(super) async fn apply_approval_party_names(
        &self,
        items: &mut [super::ApprovalListItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = crate::support::dedup_sorted(
            items
                .iter()
                .filter_map(|item| item.document_summary.as_ref())
                .filter_map(|summary| submitter_section(&summary.summary_sections))
                .map(|section| section.value.clone())
                .filter(|value| is_account_id(value)),
        );
        if ids.is_empty() {
            return Ok(());
        }
        let names = self.object_reader().load_account_names(&ids, executor).await?;
        for summary in items.iter_mut().filter_map(|item| item.document_summary.as_mut()) {
            apply_submitter_name(&mut summary.summary_sections, &names);
        }
        Ok(())
    }
}

impl WorkbenchFactsReader<'_> {
    // 读取跨域审批人员名称时，按账号 ID 批量读取姓名。
    //
    // 调用参数：
    // * `owner_ids` - 去重后的账号 ID
    //
    // 返回账号 ID 到姓名的映射。
    //
    // 账号查询失败时返回仓储错误。
    pub(super) async fn load_account_names(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(self
            .db
            .accounts()
            .list_work_item_party_accounts(owner_ids, executor)
            .await?
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect())
    }
}

// 处理跨域审批人员名称时，找到简报里的提交人段。
//
// 调用参数：
// * `sections` - 简报键值段
//
// 存在「提交人」段时返回该段。
fn submitter_section(sections: &[WorkItemSummarySection]) -> Option<&WorkItemSummarySection> {
    sections.iter().find(|section| section.label == SUBMITTER_LABEL)
}

// 组装跨域审批人员名称时，把简报提交人 ID 换成姓名；换不出姓名就摘掉该段。
//
// 调用参数：
// * `sections` - 简报键值段
// * `names` - 账号 ID 到姓名
//
// 无。已经是姓名的段保持原样。
fn apply_submitter_name(sections: &mut Vec<WorkItemSummarySection>, names: &HashMap<String, String>) {
    let Some(index) = sections.iter().position(|section| section.label == SUBMITTER_LABEL) else {
        return;
    };
    if !is_account_id(&sections[index].value) {
        return;
    }
    match names
        .get(&sections[index].value)
        .map(String::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != sections[index].value.trim())
    {
        Some(name) => sections[index].value = name.to_string(),
        None => {
            sections.remove(index);
        },
    }
}

// 确定跨域审批人员名称的适用条件时，判断取值是否为账号稳定 ID：24 位 ObjectId 或本系统 32 位 hex。
//
// 调用参数：
// * `value` - 简报段取值
//
// 是账号 ID 时返回 `true`。
fn is_account_id(value: &str) -> bool {
    let value = value.trim();
    matches!(value.len(), 24 | 32) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submitter_identity_is_replaced_or_removed_without_leaking_raw_ids() {
        let id = "0123456789abcdef01234567";
        let section = || WorkItemSummarySection::new("提交人".into(), id.into());
        let mut sections = vec![section()];
        apply_submitter_name(&mut sections, &HashMap::from([(id.into(), " 陈国平 ".into())]));
        assert_eq!(sections[0].value, "陈国平");

        for names in [HashMap::new(), HashMap::from([(id.into(), id.into())])] {
            let mut sections = vec![section()];
            apply_submitter_name(&mut sections, &names);
            assert!(sections.is_empty());
        }
    }
}
