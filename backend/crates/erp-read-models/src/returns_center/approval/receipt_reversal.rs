use erp_returns::entity::returns::ReceiptReversalStatus;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::policy::ApprovalRequirement;

use super::super::dto::{
    DocumentApprovalHistoryPageView, DocumentApprovalInstanceView, DocumentApprovalView,
};
use super::definition_view_from_binding;

/// 由绑定与可选实例事实构造回款冲正只读审批结构。
///
/// 创建后未提交只返回绑定定义；客户端不得据此选择定义或审批人。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `status` - 当前业务状态
///
/// # 返回
/// 返回有界只读审批结构。
pub fn receipt_reversal_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: ReceiptReversalStatus,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView { next_cursor: None, has_more: false },
        allowed_actions: receipt_reversal_allowed_actions(status),
    }
}

/// 回款冲正详情允许的审批相关动作。不含选择定义或审批人。
fn receipt_reversal_allowed_actions(status: ReceiptReversalStatus) -> Vec<String> {
    match status {
        ReceiptReversalStatus::Draft => vec!["SUBMIT".to_string()],
        ReceiptReversalStatus::InApproval => vec!["CANCEL".to_string()],
        ReceiptReversalStatus::Posted | ReceiptReversalStatus::Reversed => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_workflow::service::approval::binding::binding_from_published;

    use super::super::RECENT_HISTORY_LIMIT;
    use super::*;
    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 2, Instant::from_unix_secs(1))
                .unwrap();
        let view = receipt_reversal_approval_view(Some(&binding), None, ReceiptReversalStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view.allowed_actions.iter().any(|item| item.contains("DEFINITION")));
        let running = receipt_reversal_approval_view(Some(&binding), None, ReceiptReversalStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }
}
