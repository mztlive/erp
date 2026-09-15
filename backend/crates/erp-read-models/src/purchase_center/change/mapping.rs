//! 采购变更实体与冻结审批绑定的展示映射。
use erp_procurement::entity::purchase_order::PurchaseChangeOrder;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;

use super::super::approval::change::document_approval_view;
use super::super::dto::PurchaseChangeOrderView;

/// 由变更单构造列表/详情视图。
///
/// # 参数
/// * `change` - 变更单
/// * `binding` - 详情时的冻结绑定；列表为空
///
/// # 返回
/// 返回视图。
pub(super) fn change_list_view(
    change: PurchaseChangeOrder,
    binding: Option<ApprovalDefinitionBinding>,
) -> PurchaseChangeOrderView {
    PurchaseChangeOrderView {
        id: change.base.id.clone(),
        purchase_order_id: change.purchase_order_id.to_string(),
        base_revision_id: change.base_revision_id.to_string(),
        reason: change.reason.clone(),
        status: change.stable.status.as_str().to_string(),
        current_submission_id: change.current_submission_id.as_ref().map(ToString::to_string),
        effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
        version: change.base.version,
        created_at: change.base.created_at,
        approval: document_approval_view(binding.as_ref(), None, change.stable.status),
    }
}
