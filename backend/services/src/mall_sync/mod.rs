//! 域 D23 `mall_sync`：mall_sales_sync_job、mall_sales_sync_cursor、mall_sales_order_snapshot、mall_sales_reconciliation_job(+_item)、master_mapping_task（页面：W17）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
