//! 域 D11 `warehouse`：warehouse、warehouse_revision、warehouse_sku_policy（页面：W14）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D11 仓储访问器（P2 填充）。
pub trait WarehouseExt: Sized {}

impl WarehouseExt for mongodb::Database {}
