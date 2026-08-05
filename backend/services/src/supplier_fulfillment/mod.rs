//! 域 D32 `supplier_fulfillment`：supplier_fulfillment_order、supplier_fulfillment_item、supplier_order_action(+_line)、supplier_order_status_history、supplier_refund_fact、supplier_refund_allocation（页面：W26）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
