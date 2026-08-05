//! 域 D31 `mall_backfill`：mall_consumption_backfill_job、mall_consumption_backfill_item（页面：W30）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D31 仓储访问器（P2 填充）。
pub trait MallBackfillExt: Sized {}

impl MallBackfillExt for mongodb::Database {}
