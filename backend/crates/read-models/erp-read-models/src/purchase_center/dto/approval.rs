//! 单据统一只读审批展示视图。

use crate::approval_runtime::DocumentApprovalView as ApprovalView;
pub use crate::approval_runtime::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView,
    DocumentApprovalHistorySummaryItemView as DocumentApprovalHistoryItemView,
    DocumentApprovalInstanceSummaryView as DocumentApprovalInstanceView, DocumentApprovalNodeView,
};

/// 单据详情审批结构，使用销售与采购的实例及历史摘要字段合同。
pub type DocumentApprovalView = ApprovalView<DocumentApprovalInstanceView, DocumentApprovalHistoryItemView>;
