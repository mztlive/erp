//! 审批跟踪列表复用责任队列的单据摘要，读取授权与操作授权保持独立。
use super::brief::assemble_brief;
use super::dto::{WorkItemBriefLine, WorkItemSummarySection};
use super::facts::{object_policy, WorkbenchObjectFact};
use super::WorkbenchReadService;
use crate::errors::Result;
use application_core::AuditActor;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::service::approval::execution::runtime_service::{
    ApprovalRuntimeService, RuntimeInstanceListCursor, RuntimeInstanceListItem, RuntimeInstanceListQuery,
};
use persistence_core::NoTransaction;
use serde::Serialize;
use std::collections::HashSet;

/// 实例授权后的单据摘要；与责任队列使用同一组字段与明细格式。
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalDocumentSummary {
    pub counterparty_label: Option<String>,
    pub impact_summary: Option<String>,
    pub list_summary: String,
    pub summary_sections: Vec<WorkItemSummarySection>,
    pub brief_lines: Vec<WorkItemBriefLine>,
    pub brief_more_count: u32,
}
/// 保留实例列表协议，仅追加单据摘要。
#[derive(Debug, Serialize)]
pub struct ApprovalListItem {
    #[serde(flatten)]
    pub instance: RuntimeInstanceListItem,
    pub document_summary: Option<ApprovalDocumentSummary>,
}
/// 已完成运行权限校验和业务摘要装配的分页结果。
pub struct ApprovalListPage {
    pub items: Vec<ApprovalListItem>,
    pub total: u64,
    pub next_cursor: Option<RuntimeInstanceListCursor>,
}
impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 先由已装配的运行服务执行原列表授权，再批量补齐相同单据摘要。
    ///
    /// # 错误
    /// 保留运行服务的权限、参数错误；摘要仓储读取失败返回错误，不伪造完整数据。
    pub async fn approval_instance_list(
        &self,
        runtime: &ApprovalRuntimeService<A>,
        actor: &AuditActor,
        query: RuntimeInstanceListQuery,
    ) -> Result<ApprovalListPage> {
        let page = runtime.instance_list(actor, query).await?;
        let keys = page
            .items
            .iter()
            .filter_map(|item| {
                let policy = object_policy(WorkItemType::DocumentApproval, item.document_type.as_deref()?)?;
                Some((policy.object_kind, item.document_id.clone()?))
            })
            .collect::<HashSet<_>>();
        let facts = self.load_object_facts(&keys, &mut NoTransaction).await?;
        let items = page
            .items
            .into_iter()
            .map(|instance| {
                let document_summary = instance
                    .document_type
                    .as_deref()
                    .and_then(|kind| object_policy(WorkItemType::DocumentApproval, kind))
                    .and_then(|policy| facts.get(&(policy.object_kind, instance.document_id.clone()?)))
                    .and_then(|fact| document_summary(fact, instance.subject_version));
                ApprovalListItem {
                    instance,
                    document_summary,
                }
            })
            .collect();
        Ok(ApprovalListPage {
            items,
            total: page.total,
            next_cursor: page.next_cursor,
        })
    }
}
/// 有提交级摘要时必须精确匹配版本，不得用当前提交覆盖历史审批。
fn document_summary(fact: &WorkbenchObjectFact, version: Option<u32>) -> Option<ApprovalDocumentSummary> {
    let subject = if fact.display.subject_briefs.is_empty() {
        None
    } else {
        Some(fact.display.subject_briefs.get(&version?.to_string())?)
    };
    let source = subject
        .and_then(|value| value.brief_source.as_ref())
        .or(fact.display.brief_source.as_ref())?;
    let brief = assemble_brief(source, None);
    Some(ApprovalDocumentSummary {
        counterparty_label: subject
            .and_then(|value| value.counterparty_label.clone())
            .or_else(|| fact.display.counterparty_label.clone()),
        impact_summary: subject
            .and_then(|value| value.impact_summary.clone())
            .or_else(|| fact.display.impact_summary.clone()),
        list_summary: brief.list_summary,
        summary_sections: brief
            .sections
            .into_iter()
            .map(|section| WorkItemSummarySection {
                label: section.label,
                value: section.value,
                numeric: section.numeric.then_some(true),
                object_id: section.object_id,
            })
            .collect(),
        brief_lines: brief
            .lines
            .into_iter()
            .map(|line| WorkItemBriefLine {
                title: line.title,
                quantity: line.quantity,
                due_label: line.due_label,
            })
            .collect(),
        brief_more_count: brief.more_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::brief::{BriefLine, BriefSection, ObjectBriefSource};
    use crate::workbench::WorkbenchSubjectDisplay;

    /// 同单据两个提交保留各自金额与明细；两个入口消费同一组摘要字段。
    #[test]
    fn shared_summary_selects_exact_submission_and_never_substitutes_latest() {
        let mut fact = WorkbenchObjectFact::from_authority(erp_workflow::ports::ObjectFact::new(
            "po",
            "采购单 PO1",
            "starter",
        ));
        for (version, amount, title) in [(1, "¥920", "礼盒"), (2, "¥1,840", "礼盒新版")] {
            let source = ObjectBriefSource {
                amount_label: Some(amount.into()),
                extra_sections: vec![BriefSection {
                    label: "付款条件".into(),
                    value: "先款 50%".into(),
                    numeric: false,
                    object_id: None,
                }],
                lines: vec![BriefLine {
                    title: title.into(),
                    quantity: Some("1 盒".into()),
                    due_label: None,
                }],
                ..Default::default()
            };
            fact.display.brief_source = Some(source.clone());
            fact.display.subject_briefs.insert(
                version.to_string(),
                WorkbenchSubjectDisplay {
                    counterparty_label: Some("供应商".into()),
                    brief_source: Some(source),
                    ..Default::default()
                },
            );
        }
        let old = document_summary(&fact, Some(1)).unwrap();
        let latest = document_summary(&fact, Some(2)).unwrap();
        assert_eq!(old.counterparty_label.as_deref(), Some("供应商"));
        assert_eq!(old.brief_lines[0].title, "礼盒");
        assert_eq!(latest.brief_lines[0].title, "礼盒新版");
        assert_eq!(
            old.summary_sections
                .iter()
                .find(|s| s.label == "含税金额")
                .unwrap()
                .value,
            "¥920"
        );
        let queue = assemble_brief(
            fact.display.subject_briefs["1"].brief_source.as_ref().unwrap(),
            None,
        );
        assert_eq!(old.summary_sections.len(), queue.sections.len());
        for (actual, expected) in old.summary_sections.iter().zip(&queue.sections) {
            assert_eq!((&actual.label, &actual.value), (&expected.label, &expected.value));
        }
        assert!(document_summary(&fact, Some(3)).is_none());
        assert!(document_summary(&fact, None).is_none());
        let json = serde_json::to_value(old).unwrap();
        assert!(json.get("allowed_actions").is_none());
    }
}
