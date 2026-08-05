//! 域 D22 `legacy_import`：legacy_import_batch、legacy_import_row、legacy_import_confirmation（页面：W18）。
//!
//! 字段字典与唯一约束见数据模型 §6.12（旧数据导入兼容层），导入失败处理见
//! §11.5，公共字段归属按 §4.3 判定：
//! - `legacy_import_batch` / `legacy_import_row` 是导入兼容层的批次与行记录，
//!   只使用 `BaseModel` 持久化元数据，状态与统计字段按 §6.12 各自建模，
//!   不硬套 StableBase；
//! - `legacy_import_confirmation` 是正式确认事实（§6.12），不设业务软删除，
//!   状态字段（`PENDING`/`CONFIRMED`/`REJECTED`/`INVALIDATED`）按 §6.12
//!   实现固定状态机（数据模型第 7 章，禁止运行时扩展）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 跨聚合不变式（确认矩阵全绿 guard、`work_item` 任务联动、重复导入预警等）
//! 属于 P3 服务层事务职责，不在实体层实现。

pub mod legacy_import_batch;
pub mod legacy_import_confirmation;
pub mod legacy_import_row;

pub use legacy_import_batch::{LegacyImportBatch, LegacyImportBatchData, LegacyImportBatchStatus};
pub use legacy_import_confirmation::{
    ConfirmationDecision, ConfirmationStatus, LegacyImportConfirmation, LegacyImportConfirmationData,
};
pub use legacy_import_row::{ImportStatus, LegacyImportRow, LegacyImportRowData, MappingStatus, ParseStatus};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    ExternalIdentityMapId, FileAssetId, LegacyImportBatchId, LegacyImportConfirmationId, LegacyImportRowId,
    SourceSystemId, WorkItemId,
};
