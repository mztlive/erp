//! 销售与采购详情共用的审批运行摘要投影。

use bpm::model::SubjectRef;
use erp_workflow::service::approval::execution::latest_rejection_reason;
use erp_workflow::service::approval::runtime_summary::{ApprovalRuntimeSummary, latest_runtime_summary};
use mongodb::Database;
use persistence_core::Executor;

use super::{
    DocumentApprovalHistoryPageView, DocumentApprovalHistorySummaryItemView,
    DocumentApprovalInstanceSummaryView,
};
use crate::Result;

/// 最新审批实例与首屏历史的展示结构。
#[derive(Default)]
pub(crate) struct DocumentApprovalSummary {
    pub instance: Option<DocumentApprovalInstanceSummaryView>,
    pub recent_history: Vec<DocumentApprovalHistorySummaryItemView>,
    pub history_page: DocumentApprovalHistoryPageView,
}

impl DocumentApprovalSummary {
    fn from_runtime(runtime: ApprovalRuntimeSummary) -> Self {
        let history = runtime.history;
        Self {
            instance: Some(DocumentApprovalInstanceSummaryView::from_runtime(
                &runtime.instance,
                runtime.current.as_ref(),
                latest_rejection_reason(&history.items),
            )),
            recent_history: history
                .items
                .iter()
                .map(DocumentApprovalHistorySummaryItemView::from_runtime)
                .collect(),
            history_page: DocumentApprovalHistoryPageView {
                next_cursor: history.next_cursor,
                has_more: history.has_more,
            },
        }
    }
}

/// 读取已授权主体的最新审批实例和有界历史并转换为详情摘要。
///
/// # 参数
/// * `db` - 审批数据库。
/// * `subject` - 已授权业务主体。
/// * `history_limit` - 本详情首屏的历史条数。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 没有实例时返回空摘要；最近驳回原因仅取首屏历史，保留原详情口径。
///
/// # 错误
/// 工作流读取失败时沿用原错误分类。
pub(crate) async fn load_approval_summary(
    db: &Database,
    subject: &SubjectRef,
    history_limit: u32,
    executor: &mut dyn Executor,
) -> Result<DocumentApprovalSummary> {
    Ok(latest_runtime_summary(db, subject, history_limit, executor)
        .await?
        .map(DocumentApprovalSummary::from_runtime)
        .unwrap_or_default())
}
