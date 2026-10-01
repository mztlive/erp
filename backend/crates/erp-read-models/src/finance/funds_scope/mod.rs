//! 资金往来共享授权映射：同一对象映射供列表、详情、汇总、候选、导出、命令复用。

mod allocation;
mod authorization;
mod guard;
mod invoice;
mod party_names;
mod payable;
mod payable_source;
mod payment;
mod receipt;
mod receivable;
mod receivable_display;
mod repository;
mod request;
mod rows;
mod source_authorization;
mod source_read;

pub use authorization::{
    FundsAccess, FundsAuthorization, FundsLinkedCondition, FundsLinkedFacts, FundsScopedResult, ensure_page,
    ensure_version, matches_linked_condition, restricted, scope_version, summarize_matched_shares,
    whole_amount,
};
pub use rows::{
    FundsCandidate, FundsPersonShare, FundsScopedPage, FundsSummaryView, ScopedCustomerReceiptRow,
    ScopedInvoiceRequestRow, ScopedInvoiceRow, ScopedPayableAccountRow, ScopedPurchaseInvoiceAllocationRow,
    ScopedReceivableAccountRow, ScopedSupplierPaymentRow,
};
pub(crate) use source_authorization::ledger_readable;
