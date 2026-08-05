//! 域 D32 `supplier_fulfillment`：supplier_fulfillment_order、supplier_fulfillment_item、supplier_order_action(+_line)、supplier_order_status_history、supplier_refund_fact、supplier_refund_allocation（页面：W26）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D32 仓储访问器（P2 填充）。
pub trait SupplierFulfillmentExt: Sized {}

impl SupplierFulfillmentExt for mongodb::Database {}
