//! 退货、退款与收付款冲正 HTTP 入口，按正式单据类型划分协议适配。

pub mod customer_refund;
pub mod draft_update;
pub mod payment_reversal;
pub mod purchase_return;
pub mod receipt_reversal;
pub mod sales_return;
pub mod supplier_refund;
