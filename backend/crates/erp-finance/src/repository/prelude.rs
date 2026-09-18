//! Extension traits for collection repositories.

pub use super::cost::{
    CostAllocationProfitLossExt, CostAllocationReadScopeExt, CostAllocationRepositoryExt,
    CostEntryProfitLossExt, CostEntryReadScopeExt, CostEntryRepositoryExt,
};
pub use super::fulfillment_facts::PayableAccountFulfillmentFactsExt;
pub use super::payable::{
    PayableAccountInvoicingExt, PayableAccountRepositoryExt, PayableAccountSettlementExt,
    PayableEntryOffsetRepositoryExt, PayableEntryRepositoryExt, PaymentAllocationRepositoryExt,
    PurchaseInvoiceAllocationRepositoryExt, SupplierPaymentRepositoryExt,
};
pub use super::receivable::{
    CustomerReceiptRepositoryExt, InvoiceRepositoryExt, ReceiptAllocationRepositoryExt,
    ReceivableAccountCustomerCenterExt, ReceivableAccountMoneyProgressExt, ReceivableAccountRepositoryExt,
    ReceivableAccountSalesOrderSummaryExt, ReceivableAccountSnapshotExt, ReceivableEntryOffsetRepositoryExt,
    ReceivableEntryRepositoryExt, ReceivableEntrySnapshotExt, SalesInvoiceAllocationRepositoryExt,
    SalesInvoiceRequestRepositoryExt,
};
