//! 域 D08 `customer`：customer_account、customer_assignment（页面：W03、W15）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D08 仓储访问器（P2 填充）。
pub trait CustomerExt: Sized {}

impl CustomerExt for mongodb::Database {}
