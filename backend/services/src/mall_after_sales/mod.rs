//! 域 D30 `mall_after_sales`：mall_after_sales_request(+_line)、mall_refund(+_line)、mall_refund_allocation、mall_balance_restoration(+_allocation)（页面：W25）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
