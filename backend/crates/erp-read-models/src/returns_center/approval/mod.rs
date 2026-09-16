//! 逆向资金单据的只读审批摘要。
mod customer_refund;
mod runtime;
pub(super) use runtime::load_runtime;
mod payment_reversal;
mod receipt_reversal;
mod supplier_refund;
pub(super) use customer_refund::document_approval_view;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
pub(super) use payment_reversal::payment_reversal_approval_view;
pub(super) use receipt_reversal::receipt_reversal_approval_view;
pub(super) use supplier_refund::supplier_refund_approval_view;

use super::dto::DocumentApprovalDefinitionView;
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
