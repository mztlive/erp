//! Extension traits for collection repositories.

pub use super::returns::{
    CustomerRefundRepositoryExt, PaymentReversalRepositoryExt, PurchaseReturnLineRepositoryExt,
    PurchaseReturnOrderRepositoryExt, ReceiptReversalRepositoryExt, SalesReturnCaseRepositoryExt,
    SalesReturnLineRepositoryExt, SupplierRefundRepositoryExt,
};
pub use super::returns_posted_totals::{
    CustomerRefundPostedTotalsExt, PaymentReversalPostedTotalsExt, ReceiptReversalPostedTotalsExt,
    SupplierRefundPostedTotalsExt,
};
