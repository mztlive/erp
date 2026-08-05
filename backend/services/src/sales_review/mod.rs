//! 域 D14 `sales_review`：sales_order_review、procurement_confirmation(+_line)、sales_change_order、sales_change_submission、sales_change_review（页面：W05、W07）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
