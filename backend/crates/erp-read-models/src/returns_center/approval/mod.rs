//! 逆向资金单据的只读审批摘要。
mod customer_refund;
mod runtime;
pub(super) use runtime::load_runtime;
mod payment_reversal;
mod receipt_reversal;
mod supplier_refund;
pub(super) use customer_refund::document_approval_view;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::document_registry::find_approval_binding;
pub(super) use payment_reversal::payment_reversal_approval_view;
use persistence_core::NoTransaction;
pub(super) use receipt_reversal::receipt_reversal_approval_view;
pub(super) use supplier_refund::supplier_refund_approval_view;

use super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};
/// 详情最近审批历史条数上限。完整历史走分页端点。
pub const RECENT_HISTORY_LIMIT: usize = 8;

/// 由冻结绑定投影定义摘要。节点详情不在单据详情展开。
fn definition_view_from_binding(binding: &ApprovalDefinitionBinding) -> DocumentApprovalDefinitionView {
    DocumentApprovalDefinitionView::new(
        binding.approval_process_definition_id.as_ref().to_string(),
        String::new(),
    )
    .with_version(binding.approval_definition_version)
}

/// 逆向资金单据只读审批：requirement 固定，动作只由草稿/审批中决定。
fn process_required_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    is_draft: bool,
    is_in_approval: bool,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: "PROCESS_REQUIRED".to_string(),
        definition: binding.map(definition_view_from_binding),
        instance,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView::default(),
        allowed_actions: process_required_allowed_actions(is_draft, is_in_approval),
    }
}

fn process_required_allowed_actions(is_draft: bool, is_in_approval: bool) -> Vec<String> {
    if is_draft {
        vec!["SUBMIT".to_string()]
    } else if is_in_approval {
        vec!["CANCEL".to_string()]
    } else {
        Vec::new()
    }
}

/// 缺注册行时把 NotFound 吞成未绑定，其它错误原样上抛。
///
/// # 参数
/// * `db` - 数据库
/// * `document_id` - 业务单据主键
///
/// # 返回
/// 返回冻结绑定；单据未注册或未绑定时为 `None`。
///
/// # 错误
/// 仓储失败等非 NotFound 错误原样返回。
pub(super) async fn optional_approval_binding(
    db: &mongodb::Database,
    document_id: &str,
) -> crate::Result<Option<ApprovalDefinitionBinding>> {
    binding_or_none(
        find_approval_binding(db, document_id, &mut NoTransaction).await.map_err(crate::Error::from),
    )
}

fn binding_or_none(
    result: crate::Result<Option<ApprovalDefinitionBinding>>,
) -> crate::Result<Option<ApprovalDefinitionBinding>> {
    match result {
        Ok(binding) => Ok(binding),
        Err(crate::Error::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_required_actions_follow_draft_and_in_approval() {
        let draft = process_required_view(None, None, true, false);
        assert_eq!(draft.requirement, "PROCESS_REQUIRED");
        assert!(draft.definition.is_none());
        assert_eq!(draft.allowed_actions, vec!["SUBMIT".to_string()]);
        let running = process_required_view(None, None, false, true);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
        let other = process_required_view(None, None, false, false);
        assert!(other.allowed_actions.is_empty());
        assert!(other.recent_history.len() <= RECENT_HISTORY_LIMIT);
    }

    #[test]
    fn missing_registry_not_found_becomes_unbound() {
        assert!(matches!(binding_or_none(Ok(None)), Ok(None)));
        assert!(matches!(binding_or_none(Err(crate::Error::NotFound("业务单据未注册".into()))), Ok(None)));
        assert!(matches!(
            binding_or_none(Err(crate::Error::ValidationError("x".into()))),
            Err(crate::Error::ValidationError(_))
        ));
    }
}
