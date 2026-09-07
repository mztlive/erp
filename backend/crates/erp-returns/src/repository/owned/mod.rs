//! 组合 persistence-core 的退货域自有仓储类型。

mod customer_refund;
mod payment_reversal;
mod purchase_return_line;
mod purchase_return_order;
mod receipt_reversal;
mod sales_return_case;
mod sales_return_line;
mod supplier_refund;

pub use customer_refund::CustomerRefundRepository;
pub use payment_reversal::PaymentReversalRepository;
pub use purchase_return_line::PurchaseReturnLineRepository;
pub use purchase_return_order::PurchaseReturnOrderRepository;
pub use receipt_reversal::ReceiptReversalRepository;
pub use sales_return_case::SalesReturnCaseRepository;
pub use sales_return_line::SalesReturnLineRepository;
pub use supplier_refund::SupplierRefundRepository;
