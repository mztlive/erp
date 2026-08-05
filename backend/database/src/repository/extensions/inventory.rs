//! 域 D17 `inventory`：stock_movement、stock_balance、stock_reservation(+_entry)、stock_adjustment(+_line)（页面：W10）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D17 仓储访问器（P2 填充）。
pub trait InventoryExt: Sized {}

impl InventoryExt for mongodb::Database {}
