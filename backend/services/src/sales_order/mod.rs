//! 域 D13 `sales_order`：sales_order(+_line)、sales_order_working_copy、sales_order_submission、sales_order_revision、goods_service_line_revision、voucher_line_revision（页面：W05）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
