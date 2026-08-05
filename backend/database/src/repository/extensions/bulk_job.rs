//! 域 D04 `bulk_job`：bulk_selection_snapshot、bulk_selection_item、background_job、background_job_item（页面：W02、W18）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D04 仓储访问器（P2 填充）。
pub trait BulkJobExt: Sized {}

impl BulkJobExt for mongodb::Database {}
