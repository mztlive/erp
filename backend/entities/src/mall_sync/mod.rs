//! 域 D23 `mall_sync`：mall_sales_sync_job、mall_sales_sync_cursor、mall_sales_order_snapshot、
//! mall_sales_reconciliation_job(+_item)、master_mapping_task（页面：W17）。
//!
//! 字段字典与唯一约束见数据模型 §6.13（第一期商城卡券销售单同步），
//! 一期快照应用不变量见 §8.4 第 2 条，公共字段归属按 §4.3 判定：
//! - `mall_sales_sync_job` / `mall_sales_reconciliation_job(+_item)` /
//!   `master_mapping_task` 是作业与差异任务，按 §6.13 字典精确建模（状态、
//!   计数、统计字段各自建模，不硬套 StableBase）；
//! - `mall_sales_sync_cursor` 是同步水位指针（每个来源商城一个），水位只前进，
//!   提供 `move_forward()` 单调推进，不允许通用 update 回退；
//! - `mall_sales_order_snapshot` 是历史快照记录，内容创建后不可修改，
//!   只允许按固定状态机推进 `mapping_status`（`update` 受限）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 水位前进、幂等与差异处理等跨聚合不变式（§8.4 第 2 条：分页全部安全
//! 持久化后才前移水位）属于 P3 服务层事务职责，不在实体层实现。

pub mod external_order_key;
pub mod mall_sales_order_snapshot;
pub mod mall_sales_reconciliation;
pub mod mall_sales_sync_cursor;
pub mod mall_sales_sync_job;
pub mod master_mapping_task;
pub mod reapply_operation;

pub use external_order_key::ExternalOrderKey;
pub use mall_sales_order_snapshot::{
    MallSalesOrderSnapshot, MallSalesOrderSnapshotData, SnapshotMappingStatus,
};
pub use mall_sales_reconciliation::{
    MallSalesReconciliationItem, MallSalesReconciliationItemData, MallSalesReconciliationJob,
    MallSalesReconciliationJobData, ReconciliationDifferenceType, ReconciliationItemStatus,
    ReconciliationJobStatus,
};
pub use mall_sales_sync_cursor::MallSalesSyncCursor;
pub use mall_sales_sync_job::{
    MallSalesSyncJob, MallSalesSyncJobData, MallSalesSyncJobStatus, MallSalesSyncJobType,
    MallSyncTriggerSource,
};
pub use master_mapping_task::{MappingTaskStatus, MappingTaskType, MasterMappingTask, MasterMappingTaskData};
pub use reapply_operation::{
    MallSnapshotReapplyOperation, MallSnapshotReapplyOperationData, ReapplyOperationStatus,
};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    MallSalesOrderSnapshotId, MallSalesReconciliationItemId, MallSalesReconciliationJobId,
    MallSalesSyncCursorId, MallSalesSyncJobId, MasterMappingTaskId, SalesOrderId, SalesOrderRevisionId,
    SourceSystemId,
};
