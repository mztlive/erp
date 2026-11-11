//! 资金往来共享授权映射：同一对象映射供列表、详情、汇总、候选、导出、命令复用。

mod allocation;
mod authorization;
mod guard;
mod invoice;
mod invoice_files;
mod invoice_rows;
mod linked;
mod party_names;
mod payable;
mod payment;
mod payment_guidance;
mod receipt;
mod receipt_display;
mod receipt_files;
mod receipt_pending;
mod receivable;
mod receivable_display;
mod repository;
mod request;
mod rows;
mod source_authorization;
mod source_read;
pub use authorization::{
    FundsAccess, FundsAuthorization, FundsLinkedCondition, FundsLinkedFacts, FundsReadContext,
    FundsScopedResult, ensure_page, ensure_version, matches_linked_condition, scope_version,
    summarize_matched_shares, whole_amount,
};
pub(crate) use repository::whole_access::WholeAccess;
pub use rows::{
    FundsCandidate, FundsPersonShare, FundsScopedPage, FundsSummaryView, ScopedCustomerReceiptRow,
    ScopedInvoiceRequestRow, ScopedInvoiceRow, ScopedPayableAccountRow, ScopedPurchaseInvoiceAllocationRow,
    ScopedReceivableAccountRow, ScopedSupplierPaymentRow,
};

mod allocation_source;
pub use allocation_source::{AllocationSource, AllocationSourceKind};
