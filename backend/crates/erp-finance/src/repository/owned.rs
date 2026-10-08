//! [`persistence_core::Repository`] 的按集合类型别名。
//!
//! 领域专用方法是泛型仓储上的扩展 trait。

pub type CostAllocationRepository<'a> = persistence_core::Repository<'a, crate::entity::cost::CostAllocation>;
pub type CostEntryRepository<'a> = persistence_core::Repository<'a, crate::entity::cost::CostEntry>;
pub type CustomerReceiptRepository<'a> =
    persistence_core::Repository<'a, crate::entity::receivable::CustomerReceipt>;
pub type InvoiceRepository<'a> = persistence_core::Repository<'a, crate::entity::receivable::Invoice>;
pub type PayableAccountRepository<'a> =
    persistence_core::Repository<'a, crate::entity::payable::PayableAccount>;
pub type PayableEntryRepository<'a> = persistence_core::Repository<'a, crate::entity::payable::PayableEntry>;
pub type PayableEntryOffsetRepository<'a> =
    persistence_core::Repository<'a, crate::entity::payable::PayableEntryOffset>;
pub type PaymentAllocationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::payable::PaymentAllocation>;
pub type PurchaseInvoiceAllocationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::payable::PurchaseInvoiceAllocation>;
pub type ReceiptAllocationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::receivable::ReceiptAllocation>;
pub type ReceivableAccountRepository<'a> =
    persistence_core::Repository<'a, crate::entity::receivable::ReceivableAccount>;
pub type ReceivableEntryRepository<'a> =
    persistence_core::Repository<'a, crate::entity::receivable::ReceivableEntry>;
pub type ReceivableEntryOffsetRepository<'a> =
    persistence_core::Repository<'a, crate::entity::receivable::ReceivableEntryOffset>;
pub type SalesInvoiceAllocationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::receivable::SalesInvoiceAllocation>;
pub type SalesInvoiceRequestRepository<'a> =
    persistence_core::Repository<'a, crate::entity::receivable::SalesInvoiceRequest>;
pub type SupplierPaymentRepository<'a> =
    persistence_core::Repository<'a, crate::entity::payable::SupplierPayment>;
