//! 财务 MongoDB 仓储、查询事实与集合访问器。

pub mod cost;
pub mod extensions;
mod fulfillment_facts;
pub mod owned;
pub mod payable;
pub mod prelude;
mod progress;
pub mod receivable;

pub use cost::{CostAllocationFilter, CostAllocationRow, CostEntryFilter, CostEntryRow, CostRepository};
pub use extensions::{CostExt, PayableExt, ReceivableExt};
pub use owned::{
    CostAllocationRepository, CostEntryRepository, CustomerReceiptRepository, InvoiceRepository,
    PayableAccountRepository, PayableEntryOffsetRepository, PayableEntryRepository,
    PaymentAllocationRepository, PurchaseInvoiceAllocationRepository, ReceiptAllocationRepository,
    ReceivableAccountRepository, ReceivableEntryOffsetRepository, ReceivableEntryRepository,
};
pub use payable::{
    PayableAccountFilter, PayableAccountRow, PayableRepository, PurchaseInvoiceAllocationFilter,
    SupplierPaymentFilter, SupplierPaymentRow,
};
pub use prelude::*;
pub use receivable::customer_center::CustomerCenterReceivableRow;
pub use receivable::{
    CustomerReceiptFilter, CustomerReceiptRow, InvoiceFilter, InvoiceRow, ReceivableAccountFilter,
    ReceivableListScope, ReceivableRepository, ScopedCustomerReceiptQuery, ScopedInvoiceQuery,
};

#[cfg(test)]
mod test_fixture;

#[cfg(test)]
mod serialization_contract;

pub mod keyword;
