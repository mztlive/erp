//! 域 D27 `projection`：sales_order_projection(+_revision、_delivery)（页面：W23）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D27 仓储访问器（P2 填充）。
pub trait ProjectionExt: Sized {}

impl ProjectionExt for mongodb::Database {}
