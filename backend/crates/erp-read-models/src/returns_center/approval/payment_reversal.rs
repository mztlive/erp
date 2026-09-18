use erp_returns::entity::returns::PaymentReversalStatus;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;

use super::super::dto::{DocumentApprovalInstanceView, DocumentApprovalView};
use super::process_required_view;

/// 由绑定与可选实例事实构造付款冲正只读审批结构。
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
pub fn payment_reversal_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: PaymentReversalStatus,
) -> DocumentApprovalView {
    process_required_view(
        binding,
        instance,
        matches!(status, PaymentReversalStatus::Draft),
        matches!(status, PaymentReversalStatus::InApproval),
    )
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
        let view = payment_reversal_approval_view(Some(&binding), None, PaymentReversalStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view.allowed_actions.iter().any(|item| item.contains("DEFINITION")));
        let running = payment_reversal_approval_view(Some(&binding), None, PaymentReversalStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }
}
