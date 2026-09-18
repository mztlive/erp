//! 退货域拥有仓储、查询投影与调用方事务内持久化。

mod extensions;
pub mod owned;
pub mod prelude;
pub mod returns;
mod returns_posted_totals;

pub use extensions::ReturnsExt;
pub use prelude::{
    CustomerRefundPostedTotalsExt, CustomerRefundRepositoryExt, PaymentReversalPostedTotalsExt,
    PaymentReversalRepositoryExt, PurchaseReturnLineRepositoryExt, PurchaseReturnOrderRepositoryExt,
    ReceiptReversalPostedTotalsExt, ReceiptReversalRepositoryExt, SalesReturnCaseRepositoryExt,
    SalesReturnLineRepositoryExt, SupplierRefundPostedTotalsExt, SupplierRefundRepositoryExt,
};

#[cfg(test)]
mod serialization_contract;
