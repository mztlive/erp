//! 域 D18 `receivable`：receivable_account、receivable_entry、receivable_funds_review、receivable_entry_offset、customer_receipt、receipt_allocation、invoice、sales_invoice_allocation（页面：W11、W13）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D18 仓储访问器（P2 填充）。
pub trait ReceivableExt: Sized {}

impl ReceivableExt for mongodb::Database {}
