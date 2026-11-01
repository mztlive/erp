//! 集合仓储的扩展 trait。

pub use super::returns::{
    CustomerRefundRepositoryExt, PaymentReversalRepositoryExt, PurchaseReturnLineRepositoryExt,
    PurchaseReturnOrderRepositoryExt, ReceiptReversalRepositoryExt, SalesReturnCaseRepositoryExt,
    SalesReturnLineRepositoryExt, SupplierRefundRepositoryExt,
};
pub use super::returns_posted_totals::{
    CustomerRefundPostedTotalsExt, PaymentReversalPostedTotalsExt, ReceiptReversalPostedTotalsExt,
    SupplierRefundPostedTotalsExt,
};
