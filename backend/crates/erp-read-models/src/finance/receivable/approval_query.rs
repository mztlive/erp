//! 客户回款完整视图的真实审批实例、版本、最后驳回和首屏历史。

use erp_finance::entity::receivable::CustomerReceipt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use mongodb::Database;
use persistence_core::Executor;

use super::approval_view::document_approval_view;
use crate::approval_runtime::load_document_runtime;
use crate::finance::dto::{
    DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView, DocumentApprovalView,
};
use crate::{Error, Result};

/// 与完整回款视图共用原执行器装配运行事实，未提交时保持空实例。
pub(super) async fn load_receipt_document_approval(
    db: &Database,
    receipt: &CustomerReceipt,
    binding: Option<&ApprovalDefinitionBinding>,
    executor: &mut dyn Executor,
) -> Result<DocumentApprovalView> {
    let mut view = document_approval_view(binding, None, receipt.status);
    let Some(runtime) =
        load_document_runtime(db, DocumentType::CustomerReceipt, &receipt.base.id, binding, executor).await?
    else {
        view.allowed_actions.retain(|action| action != "CANCEL");
        return Ok(view);
    };
    if runtime.instance.subject_version.as_deref()
        != Some(receipt.approval_subject_version.to_string().as_str())
    {
        return Err(Error::ConflictError("回款审批实例与原单提交版本不一致".into()));
    }
    if !runtime.cancellable {
        view.allowed_actions.retain(|action| action != "CANCEL");
    }
    view.instance = Some(runtime.instance);
    view.recent_history =
        runtime.history.items.into_iter().map(DocumentApprovalHistoryItemView::from).collect();
    view.history_page = DocumentApprovalHistoryPageView {
        next_cursor: runtime.history.next_cursor,
        has_more: runtime.history.has_more,
    };
    Ok(view)
}
