//! 域 D21 `returns`：sales_return_case、sales_return_line、purchase_return_order、purchase_return_line、customer_refund、supplier_refund、receipt_reversal、payment_reversal（页面：W05、W09、W11、W12）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D21 仓储访问器（P2 填充）。
pub trait ReturnsExt: Sized {}

impl ReturnsExt for mongodb::Database {}
