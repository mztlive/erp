//! 域 D23 `mall_sync` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。
//!
//! 快照落盘契约（数据模型 §6.13）：来源单号、商城更新时间、状态码、规范化快照
//! 由同步调用方提供；来源商城与观察时间由服务端从作业上下文注入，禁止客户端
//! 伪造来源归属。

mod common;
mod mapping_task;
mod reconciliation;
mod snapshot;
mod sync_job;

pub use common::{PageParams, PageView, SortDir};
pub use mapping_task::{
    ConfirmMappingBusinessResult, ConfirmMappingCommand, ConfirmMappingResult,
    CreateMasterMappingTaskRequest, GovernanceActionResult, MappingActionBlockerView,
    MappingCandidateTargetView, MappingCurrentTargetView, MappingResolutionHistoryView,
    MappingSourceEvidenceView, MappingTaskWorkItemView, MasterMappingTaskDetailParams,
    MasterMappingTaskListParams, MasterMappingTaskView, OwnerRoutingState, ReapplyMallSnapshotCommand,
    ReapplyOperationView, RequestSourceFixCommand, RequestSourceFixResult,
};
pub use reconciliation::{
    CreateMallSalesReconciliationJobRequest, MallSalesReconciliationItemListParams,
    MallSalesReconciliationItemView, MallSalesReconciliationJobListParams, MallSalesReconciliationJobView,
    ReconciliationItemRequest, ResolveItemKind, ResolveMallSalesReconciliationItemRequest,
};
pub use snapshot::{
    IngestMallSalesOrderSnapshotsRequest, IngestMallSalesOrderSnapshotsResult,
    MallSalesOrderSnapshotListParams, MallSalesOrderSnapshotView, SnapshotItemRequest,
};
pub use sync_job::{
    CompleteMallSalesSyncJobRequest, MallSalesSyncCursorView, MallSalesSyncJobListParams,
    MallSalesSyncJobView, RetryMallSalesSyncJobRequest, SyncJobOutcome, TriggerMallSyncCommand,
};

#[allow(unused_imports)]
pub(crate) use common::{
    normalize_sort, SALES_ORDER_CUSTOMER_MISSING_MESSAGE, SALES_ORDER_NOT_FOUND_MESSAGE,
    SOURCE_SYSTEM_NOT_FOUND_MESSAGE,
};
#[allow(unused_imports)]
pub(crate) use mapping_task::{MasterMappingTaskListQuery, MASTER_MAPPING_TASK_SORT_FIELDS};
#[allow(unused_imports)]
pub(crate) use reconciliation::{
    MallSalesReconciliationItemListQuery, MallSalesReconciliationJobListQuery,
    MALL_SALES_RECONCILIATION_ITEM_SORT_FIELDS, MALL_SALES_RECONCILIATION_JOB_SORT_FIELDS,
};
#[allow(unused_imports)]
pub(crate) use snapshot::{MallSalesOrderSnapshotListQuery, MALL_SALES_ORDER_SNAPSHOT_SORT_FIELDS};
#[allow(unused_imports)]
pub(crate) use sync_job::{MallSalesSyncJobListQuery, MALL_SALES_SYNC_JOB_SORT_FIELDS};
