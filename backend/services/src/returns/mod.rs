//! 域 D21 `returns`：sales_return_case、sales_return_line、purchase_return_order、purchase_return_line、customer_refund、supplier_refund、receipt_reversal、payment_reversal（页面：W05、W09、W11、W12）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
