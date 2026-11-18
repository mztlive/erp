//! 单据详情共用的审批运行事实；调用方负责完整业务对象读取授权。

mod definition;
mod dto;
mod presentation;
mod summary;
mod summary_query;
pub use definition::{DocumentApprovalNodeView, binding_route};
mod query;

pub use dto::{DocumentApprovalHistoryItemView, DocumentApprovalInstanceView};
use erp_workflow::service::approval::execution::RuntimeHistoryPage;
pub use presentation::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalView,
};
pub(crate) use query::{load_document_runtime, load_document_runtime_for_instance};
pub use summary::{DocumentApprovalHistorySummaryItemView, DocumentApprovalInstanceSummaryView};
pub(crate) use summary_query::load_approval_summary;

/// 与同一业务读取快照绑定的实际运行摘要和有界历史。
pub(crate) struct DocumentRuntime {
    pub instance: DocumentApprovalInstanceView,
    pub history: RuntimeHistoryPage,
    pub cancellable: bool,
}
