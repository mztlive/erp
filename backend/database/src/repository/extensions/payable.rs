//! 域 D19 `payable`：payable_account、payable_entry、payable_entry_offset、supplier_payment、payment_allocation、purchase_invoice_allocation（页面：W12）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D19 仓储访问器（P2 填充）。
pub trait PayableExt: Sized {}

impl PayableExt for mongodb::Database {}
