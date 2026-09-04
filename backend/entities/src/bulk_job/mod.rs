//! 域 D04 `bulk_job`：bulk_selection_snapshot、bulk_selection_item、background_job、background_job_item（页面：W02、W18）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与必需约束见数据模型 §6.1；公共字段归属按 §4.3 判定：
//! - `bulk_selection_snapshot` 是「批量预览时冻结目标、截止水位和逐项版本」的
//!   一次性工作结构（有状态、可失效），不属稳定基础资料或正式事实 → 只用
//!   `BaseModel` 持久化元数据，`status` 状态机与审计字段按 §6.1 各自建模；
//! - `background_job` 是后台任务中心的统一注册表，不替代领域任务强类型表（§6.1）；
//! - 本域四张表的跨行/跨表不变量（快照确认后冻结、逐项重验权限/版本、计数与
//!   逐项一致、`job_no`/`request_id`/`(background_job_id, item_no)` 唯一）由
//!   P2 索引与 P3 事务实现，实体层只实现单行不变量。

mod aggregate;
pub mod background_job;
pub mod background_job_item;
pub mod bulk_selection_item;
pub mod bulk_selection_snapshot;
mod import_result_batch;
pub mod legacy_import_job;
pub mod supplier_governance_job;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{BackgroundJobId, BackgroundJobItemId, BulkSelectionItemId, BulkSelectionSnapshotId};
pub use aggregate::{
    BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobItemDraft, BulkSelectionItemDraft,
    BulkSelectionSnapshotAggregate, BulkSelectionSnapshotAggregateData,
};
pub use background_job::{BackgroundJob, BackgroundJobData, JobStatus, JobType, JobUpdate};
pub use background_job_item::{BackgroundJobItem, BackgroundJobItemData, ItemStatus};
pub use bulk_selection_item::{BulkSelectionItem, BulkSelectionItemData, SelectionItemStatus};
pub use bulk_selection_snapshot::{
    BulkSelectionSnapshot, BulkSelectionSnapshotData, SelectionStatus, SelectionType,
};
pub use legacy_import_job::{
    legacy_import_job_no, LEGACY_IMPORT_DOMAIN_JOB_TYPE, LEGACY_IMPORT_JOB_NO_PREFIX,
};
pub use supplier_governance_job::{
    SupplierGovernanceJobKind, SupplierGovernanceJobSpec, SUPPLIER_CATALOG_SYNC_JOB_TYPE,
    SUPPLIER_HEALTH_CHECK_JOB_TYPE,
};
