//! 域 D34 `integration_ops` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。
//!
//! 业务写入口只有 W29 强类型任务动作、任务完成与直接对账决定；责任退回、转交、
//! 关闭只走 W02，共享层外不得保留同义 DTO。

mod common;
mod error_task;
mod inbox_message;
mod reconciliation_difference;
mod task_decision;

#[allow(unused_imports)]
pub(crate) use common::normalize_sort;
#[allow(unused_imports)]
pub use common::{PageParams, PageView, SortDir};
pub use error_task::{
    ActionBlockerView, CreateErrorTaskRequest, ErrorTaskDetailView, ErrorTaskListParams, ErrorTaskView,
};
#[allow(unused_imports)]
pub(crate) use error_task::{ErrorTaskListQuery, ERROR_TASK_SORT_FIELDS};
pub use inbox_message::{
    InboxMessageListParams, InboxMessageListView, InboxMessageView, RegisterInboxMessageRequest,
    WriteBackInboxResultRequest, WriteBackOutcome,
};
#[allow(unused_imports)]
pub(crate) use inbox_message::{InboxMessageListQuery, INBOX_MESSAGE_SORT_FIELDS};
pub use reconciliation_difference::{
    CreateDifferenceRequest, DifferenceDetailView, DifferenceListParams, DifferenceView, ResolutionView,
};
#[allow(unused_imports)]
pub(crate) use reconciliation_difference::{DifferenceListQuery, DIFFERENCE_SORT_FIELDS};
pub use task_decision::{
    ControlledEvidenceKind, ControlledEvidenceRef, DifferenceReasonCode, DirectReconciliationCommand,
    DirectReconciliationConclusion, DirectReconciliationDecision, DirectReconciliationResult,
    DirectReconciliationStatus, EvidencePolicyKey, IntegrationActionOutcome, IntegrationItemType,
    IntegrationNonTerminalTaskAction, IntegrationResolutionReasonCode, IntegrationTaskActionCommand,
    IntegrationTaskActionEvidence, IntegrationTaskActionKind, IntegrationTaskActionResult,
    IntegrationTaskCompletionCommand, IntegrationTaskCompletionDecision, IntegrationTaskCompletionKind,
    IntegrationTaskCompletionResult, IntegrationWorkItemStatus, ReconciliationReasonRegistryView,
    RegisteredReconciliationReasonView, ResolutionEvidencePolicyView, ReviewerSeparation,
};
