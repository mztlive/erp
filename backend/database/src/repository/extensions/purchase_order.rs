//! 域 D15 `purchase_order`：purchase_order、purchase_order_submission、purchase_order_revision、purchase_line_sales_allocation、purchase_change_order 等（页面：W08）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D15 仓储访问器（P2 填充）。
pub trait PurchaseOrderExt: Sized {}

impl PurchaseOrderExt for mongodb::Database {}
