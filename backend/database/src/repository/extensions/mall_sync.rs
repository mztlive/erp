//! 域 D23 `mall_sync`：mall_sales_sync_job、mall_sales_sync_cursor、mall_sales_order_snapshot、mall_sales_reconciliation_job(+_item)、master_mapping_task（页面：W17）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D23 仓储访问器（P2 填充）。
pub trait MallSyncExt: Sized {}

impl MallSyncExt for mongodb::Database {}
