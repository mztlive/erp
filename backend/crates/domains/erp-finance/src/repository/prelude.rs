//! 集合仓储扩展 trait。

pub use super::cost::{
    CostAllocationProfitLossExt, CostAllocationReadScopeExt, CostAllocationRepositoryExt,
    CostEntryProfitLossExt, CostEntryReadScopeExt, CostEntryRepositoryExt,
};
pub use super::fulfillment_facts::PayableAccountFulfillmentFactsExt;
pub use super::payable::payment_request::PurchasePaymentRequestRepositoryExt;
pub use super::payable::{
    PayableAccountInvoicingExt, PayableAccountRepositoryExt, PayableAccountReservationExt,
    PayableAccountSettlementExt, PayableEntryOffsetRepositoryExt, PayableEntryRepositoryExt,
    PaymentAllocationRepositoryExt, PurchaseInvoiceAllocationRepositoryExt, SupplierPaymentRepositoryExt,
};
pub use super::receivable::{
    CustomerReceiptRepositoryExt, InvoiceRepositoryExt, ReceiptAllocationRepositoryExt,
    ReceiptCapacityRepositoryExt, ReceivableAccountCustomerCenterExt, ReceivableAccountInvoicingExt,
    ReceivableAccountMoneyProgressExt, ReceivableAccountRepositoryExt, ReceivableAccountSalesOrderSummaryExt,
    ReceivableAccountSettlementExt, ReceivableAccountSnapshotExt, ReceivableEntryOffsetRepositoryExt,
    ReceivableEntryRepositoryExt, ReceivableEntrySnapshotExt, SalesInvoiceAllocationRepositoryExt,
    SalesInvoiceRequestRepositoryExt,
};
