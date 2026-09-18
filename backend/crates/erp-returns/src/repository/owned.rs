//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type CustomerRefundRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::CustomerRefund>;

pub type PaymentReversalRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::PaymentReversal>;

pub type PurchaseReturnLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::PurchaseReturnLine>;

pub type PurchaseReturnOrderRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::PurchaseReturnOrder>;

pub type ReceiptReversalRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::ReceiptReversal>;

pub type SalesReturnCaseRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::SalesReturnCase>;

pub type SalesReturnLineRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::SalesReturnLine>;

pub type SupplierRefundRepository<'a> =
    persistence_core::Repository<'a, crate::entity::returns::SupplierRefund>;
