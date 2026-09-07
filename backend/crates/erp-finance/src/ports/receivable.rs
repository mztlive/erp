//! Minimal sales snapshot fields combined with finance facts for receivable decisions.

use crate::entity::receivable::{
    CustomerReceipt, Invoice, ReceiptAllocation, ReceivableEntry, ReceivableFundsReview,
    SalesInvoiceAllocation,
};

/// W13 校验所需的当前应收、票款与复核链事实快照。
pub struct CardFundsSnapshot {
    pub current_sales_order_revision_id: String,
    pub sales_order_no: String,
    pub sales_order_revision_no: u32,
    pub sales_order_snapshot_at: u64,
    pub customer_name: String,
    pub counterparty_party_name: Option<String>,
    pub entries: Vec<ReceivableEntry>,
    pub reviews: Vec<ReceivableFundsReview>,
    pub receipt_allocations: Vec<ReceiptAllocation>,
    pub invoice_allocations: Vec<SalesInvoiceAllocation>,
    pub receipts: Vec<CustomerReceipt>,
    pub invoices: Vec<Invoice>,
}
