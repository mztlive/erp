use erp_inventory::{
    CancelStockAdjustmentApprovalTokenView, DocumentApprovalDefinitionView, DocumentApprovalHistoryItemView,
    DocumentApprovalHistoryPageView, DocumentApprovalInstanceView, DocumentApprovalView,
    StockAdjustmentState, SubmitStockAdjustmentApprovalTokenView,
};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::policy::ApprovalRequirement;

pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 由绑定、运行摘要、历史和调用人提交/撤回令牌构造只读审批结构。
///
/// # 参数
/// * `binding` - 冻结定义绑定；为空时定义摘要为空。
/// * `instance` - 审批实例摘要；没有实例时为空。
/// * `recent_history` - 最近历史条目。
/// * `history_page` - 历史分页游标。
/// * `status` - 调整单状态，用于投影允许动作。
/// * `submit_command` - 当前调用人的提交令牌；为空表示不可提交。
/// * `cancel_command` - 当前调用人的撤回令牌；为空表示不可撤回。
///
/// # 返回
/// 返回只读 `DocumentApprovalView`。允许动作只在对应令牌存在时包含提交或撤回。
///
/// # 错误
/// 不返回错误。
pub fn document_approval_view_with_history(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    recent_history: Vec<DocumentApprovalHistoryItemView>,
    history_page: DocumentApprovalHistoryPageView,
    status: StockAdjustmentState,
    submit_command: Option<SubmitStockAdjustmentApprovalTokenView>,
    cancel_command: Option<CancelStockAdjustmentApprovalTokenView>,
) -> DocumentApprovalView {
    let can_submit = submit_command.is_some();
    let can_cancel = cancel_command.is_some();
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history,
        history_page,
        allowed_actions: allowed_document_actions(status, can_submit, can_cancel),
        submit_command,
        cancel_command,
    }
}

/// 由冻结绑定投影定义摘要。节点详情不在单据详情展开。
fn definition_view_from_binding(binding: &ApprovalDefinitionBinding) -> DocumentApprovalDefinitionView {
    DocumentApprovalDefinitionView {
        id: binding.approval_process_definition_id.as_ref().to_string(),
        name: String::new(),
        version: binding.approval_definition_version,
        nodes: Vec::new(),
    }
}

/// 单据详情允许的审批相关动作。不含选择定义或审批人。
fn allowed_document_actions(status: StockAdjustmentState, can_submit: bool, can_cancel: bool) -> Vec<String> {
    match status {
        StockAdjustmentState::Draft if can_submit => vec!["SUBMIT".to_string()],
        StockAdjustmentState::Draft => Vec::new(),
        StockAdjustmentState::InApproval if can_cancel => vec!["CANCEL".to_string()],
        StockAdjustmentState::InApproval => Vec::new(),
        StockAdjustmentState::Posted
        | StockAdjustmentState::Reversed
        | StockAdjustmentState::PendingWarehouseReview
        | StockAdjustmentState::PendingFinanceReview
        | StockAdjustmentState::Rejected => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_workflow::service::approval::binding::binding_from_published;

    use super::*;

    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 2, Instant::from_unix_secs(1))
                .unwrap();
        let view = document_approval_view_with_history(
            Some(&binding),
            None,
            Vec::new(),
            DocumentApprovalHistoryPageView { next_cursor: None, has_more: false },
            StockAdjustmentState::Draft,
            Some(SubmitStockAdjustmentApprovalTokenView {
                expected_version: "1".to_string(),
                expected_subject_version: "1".to_string(),
            }),
            None,
        );
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert_eq!(view.definition.as_ref().unwrap().version, 2);
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert!(!view.history_page.has_more);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view.allowed_actions.iter().any(|item| item.contains("DEFINITION")));
        assert!(!view.allowed_actions.iter().any(|item| item.contains("ASSIGNEE")));
        let running = document_approval_view_with_history(
            Some(&binding),
            None,
            Vec::new(),
            DocumentApprovalHistoryPageView { next_cursor: None, has_more: false },
            StockAdjustmentState::InApproval,
            None,
            None,
        );
        assert!(running.allowed_actions.is_empty());
        let authorized = document_approval_view_with_history(
            Some(&binding),
            None,
            Vec::new(),
            DocumentApprovalHistoryPageView { next_cursor: None, has_more: false },
            StockAdjustmentState::InApproval,
            None,
            Some(CancelStockAdjustmentApprovalTokenView {
                expected_version: "7".to_string(),
                approval_process_instance_id: "instance-1".to_string(),
                expected_subject_version: "2".to_string(),
                expected_instance_version: "3".to_string(),
                expected_execution_version: "4".to_string(),
                expected_task_version: Some("5".to_string()),
            }),
        );
        assert_eq!(authorized.allowed_actions, vec!["CANCEL".to_string()]);
    }
}
