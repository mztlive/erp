//! 域 D18 `receivable`：receivable_account、receivable_entry、receivable_funds_review、receivable_entry_offset、customer_receipt、receipt_allocation、invoice、sales_invoice_allocation（页面：W11、W13）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
