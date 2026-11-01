//! [`persistence_core::Repository`] 的集合别名。
//!
//! 领域方法是泛型仓储上的扩展 trait。

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
