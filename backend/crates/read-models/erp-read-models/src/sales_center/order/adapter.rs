//! Approval definition and history views based on frozen workflow facts.
use erp_sales::entity::sales_order::{CommercialStatus, ReviewStatus};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;

use super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView,
    DocumentApprovalInstanceView, DocumentApprovalView,
};
pub(super) const RECENT_HISTORY_LIMIT: usize = 8;
/// 由绑定与可选实例事实构造只读审批结构。
///
/// 创建后未提交只返回绑定定义；运行时不得按采购确认用途分支。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `commercial` - 当前商业主状态
/// * `review` - 当前审核轨
///
/// # 返回
/// 返回有界只读审批结构，历史为空。
#[cfg(test)]
fn document_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    commercial: CommercialStatus,
    review: ReviewStatus,
) -> DocumentApprovalView {
    document_approval_view_with_history(
        binding,
        instance,
        Vec::new(),
        DocumentApprovalHistoryPageView::default(),
        commercial,
        review,
    )
}

/// 由绑定、运行实例与有界历史构造只读审批结构。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `recent_history` - 有界最近历史，调用方负责截断
/// * `history_page` - 完整历史分页游标
/// * `commercial` - 当前商业主状态
/// * `review` - 当前审核轨
///
/// # 返回
/// 返回有界只读审批结构。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `recent_history` 不得超过 [`RECENT_HISTORY_LIMIT`]；超出部分走分页端点。
pub(super) fn document_approval_view_with_history(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    recent_history: Vec<DocumentApprovalHistoryItemView>,
    history_page: DocumentApprovalHistoryPageView,
    commercial: CommercialStatus,
    review: ReviewStatus,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: "PROCESS_REQUIRED".to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history,
        history_page,
        allowed_actions: allowed_document_actions(commercial, review),
    }
}

/// 由冻结绑定投影定义摘要。节点详情不在单据详情展开。
fn definition_view_from_binding(binding: &ApprovalDefinitionBinding) -> DocumentApprovalDefinitionView {
    DocumentApprovalDefinitionView::new(
        binding.approval_process_definition_id.as_ref().to_string(),
        String::new(),
    )
    .with_version(binding.approval_definition_version)
}

/// 单据详情允许的审批相关动作。不含选择定义或审批人。
fn allowed_document_actions(commercial: CommercialStatus, review: ReviewStatus) -> Vec<String> {
    match (commercial, review) {
        (CommercialStatus::Draft, ReviewStatus::NotSubmitted) => vec!["SUBMIT".to_string()],
        (CommercialStatus::PendingReview, ReviewStatus::InApproval) => vec!["CANCEL".to_string()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_workflow::service::approval::binding::binding_from_published;

    use super::*;
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 2, Instant::from_unix_secs(1))
                .unwrap();
        let view =
            document_approval_view(Some(&binding), None, CommercialStatus::Draft, ReviewStatus::NotSubmitted);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view.allowed_actions.iter().any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(
            Some(&binding),
            None,
            CommercialStatus::PendingReview,
            ReviewStatus::InApproval,
        );
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
        let with_history = document_approval_view_with_history(
            Some(&binding),
            Some(
                DocumentApprovalInstanceView::new("inst-1".into(), "RUNNING".into())
                    .with_current_node(Some("procurement_confirm".into()))
                    .with_current_node_name(Some("采购确认".into()))
                    .with_current_assignee(Some("u1".into()))
                    .with_current_assignee_name(Some("李思勇".into()))
                    .with_process_version(Some(2)),
            ),
            vec![
                DocumentApprovalHistoryItemView::new(
                    "exec-1".into(),
                    "procurement_confirm".into(),
                    "采购确认".into(),
                    "ACTIVE".into(),
                )
                .with_assignee_name(Some("李思勇".into())),
            ],
            DocumentApprovalHistoryPageView::default(),
            CommercialStatus::PendingReview,
            ReviewStatus::InApproval,
        );
        assert_eq!(with_history.instance.as_ref().unwrap().id, "inst-1");
        assert_eq!(with_history.recent_history.len(), 1);
        assert_eq!(with_history.recent_history[0].node_name, "采购确认");
        assert!(with_history.recent_history.len() <= RECENT_HISTORY_LIMIT);
    }
}
