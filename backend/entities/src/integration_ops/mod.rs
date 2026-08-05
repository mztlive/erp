//! 域 D34 `integration_ops`：inbox_message、integration_error_task、reconciliation_difference(+_resolution)（页面：W29）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 P0 公共基元。
//! 字段字典见数据模型 §6.21；公共字段归属按 §4.3 判定：
//! - `inbox_message` 是已接收外部消息的普通入站记录（消息事实，含幂等键与来源引用），
//!   字典未含 `FactBase` 语义字段 → 只用 `BaseModel`；处理状态按 §6.21 固化取值，
//!   投递状态由 `integration_error_task.status` 表达（§7.7，不另设消息投递状态机）；
//! - `integration_error_task` 是集成任务（普通表），`status` 按 §7.7 实现固定状态机，
//!   终态（已解决/已关闭）无出边，测试采用逐边定向断言；
//! - `reconciliation_difference` 是正式差异事实、`reconciliation_difference_resolution`
//!   是不可变解决记录（§4.5.1 正式事实不设业务软删除，解决记录只追加不更新）；
//!   差异发现时间由字典 `created_at` 承载（≡ `BaseModel.created_at`），不另设
//!   `occurred_at`/`recorded_by` 等字段。
//!
//! 业务规则来源：§6.21（字段字典与约束）、§7.7（投递状态由错误任务表达）、
//! §8.4 第 3 条（inbox 消息去重与业务事实键幂等）、§4.2/§4.3/§4.5（定点数值、
//! 公共字段、事实不软删）、`erp-phase-2.md` §13（接口治理、重试与人工补偿）、
//! `erp-mall-data-mapping.md` §10.4.1（商城关键事实共同信封）。

mod inbox_message;
mod integration_error_task;
mod reconciliation_difference;
mod reconciliation_difference_resolution;

pub use inbox_message::*;
pub use integration_error_task::*;
pub use reconciliation_difference::*;
pub use reconciliation_difference_resolution::*;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    InboxMessageId, IntegrationErrorTaskId, ReconciliationDifferenceId, ReconciliationDifferenceResolutionId,
    SourceSystemId,
};
