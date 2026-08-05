//! 域 D20 `cost`：cost_entry、cost_allocation（页面：W16）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D20 仓储访问器（P2 填充）。
pub trait CostExt: Sized {}

impl CostExt for mongodb::Database {}
