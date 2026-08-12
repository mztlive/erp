//! 域 D34 `integration_ops` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。
//!
//! 与 `erp-client/features/integration-errors` 契约对齐（P3 §4.7）：
//! - REPLAY 请求类型**不包含** `originalActionIdempotencyKey` 字段，并以
//!   `#[serde(deny_unknown_fields)]` 强制拒绝客户端传入原键（服务端锁定原键）；
//! - QUERY 结果区分「已受理/明确无结果/仍未知」，只有 `no_result_confirmed`
//!   才可能开放 REPLAY（§7.7、W29 §8.2）；
//! - 差异终结只接受固定原因枚举（W29 §7：`SOURCE_CORRECTED_AND_REATTRIBUTED` /
//!   `BUSINESS_CONFIRMED_NO_ERROR` / `COMPENSATION_CLOSED`），禁止自由文本原因。

mod common;
mod error_task;
mod inbox_message;
mod reconciliation_difference;

#[allow(unused_imports)]
pub(crate) use common::normalize_sort;
#[allow(unused_imports)]
pub use common::{PageParams, PageView, SortDir};
pub use error_task::{
    CloseErrorTaskRequest, CloseReason, CreateErrorTaskRequest, ErrorTaskDetailView, ErrorTaskListParams,
    ErrorTaskView, HoldErrorTaskRequest, HoldKind, QueryOriginalResultRequest, QueryOutcome,
    ReplayOriginalRequest, ReplayResultView, ResolveErrorTaskRequest, TransferErrorTaskRequest,
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
    CreateDifferenceRequest, DifferenceActionView, DifferenceConclusion, DifferenceDetailView,
    DifferenceListParams, DifferenceProcessAction, DifferenceReasonCode, DifferenceView,
    ProcessDifferenceRequest, ResolutionView, ResolveDifferenceRequest,
};
#[allow(unused_imports)]
pub(crate) use reconciliation_difference::{DifferenceListQuery, DIFFERENCE_SORT_FIELDS};
