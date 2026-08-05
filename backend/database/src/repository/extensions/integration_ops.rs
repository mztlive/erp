//! 域 D34 `integration_ops`：inbox_message、integration_error_task、reconciliation_difference(+_resolution)（页面：W29）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D34 仓储访问器（P2 填充）。
pub trait IntegrationOpsExt: Sized {}

impl IntegrationOpsExt for mongodb::Database {}
