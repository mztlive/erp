//! 冻结绑定与业务状态的采购审批只读投影。
use erp_procurement::entity::purchase_order::PurchaseChangeOrderStatus;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::policy::ApprovalRequirement;

use super::super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};
/// 由绑定与可选实例事实构造只读审批结构。
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
pub fn document_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: PurchaseChangeOrderStatus,
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
        history_page: DocumentApprovalHistoryPageView::default(),
        allowed_actions: allowed_document_actions(status),
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
fn allowed_document_actions(status: PurchaseChangeOrderStatus) -> Vec<String> {
    match status {
        PurchaseChangeOrderStatus::Draft => vec!["SUBMIT".to_string()],
        PurchaseChangeOrderStatus::InApproval => vec!["CANCEL".to_string()],
        PurchaseChangeOrderStatus::Effective | PurchaseChangeOrderStatus::Voided => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_workflow::service::approval::binding::binding_from_published;

    use super::*;
    const RECENT_HISTORY_LIMIT: usize = 8;
    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 2, Instant::from_unix_secs(1))
                .unwrap();
        let view = document_approval_view(Some(&binding), None, PurchaseChangeOrderStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view.allowed_actions.iter().any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(Some(&binding), None, PurchaseChangeOrderStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }
}
