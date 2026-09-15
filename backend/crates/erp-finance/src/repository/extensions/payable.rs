//! 域 D19 `payable` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS` 等值。
//!
//! `invoice` 由 D18 拥有，D19 在 P3 通过 `ReceivableExt::invoices()` 复用，
//! 本域只拥有 `purchase_invoice_allocation`（domains.md §3）。

use mongodb::Database;

use super::super::payable::{
    PayableAccountFilter, PayableRepository, PurchaseInvoiceAllocationFilter, SupplierPaymentFilter,
};
use crate::repository::owned::{
    PayableAccountRepository, PayableEntryOffsetRepository, PayableEntryRepository,
    PaymentAllocationRepository, PurchaseInvoiceAllocationRepository, SupplierPaymentRepository,
};

/// 域 D19 仓储访问器。
pub trait PayableExt {
    /// `payable_account` 集合名。
    const PAYABLE_ACCOUNTS: &'static str = "payable_accounts";
    /// `payable_entry` 集合名。
    const PAYABLE_ENTRIES: &'static str = "payable_entries";
    /// `payable_entry_offset` 集合名。
    const PAYABLE_ENTRY_OFFSETS: &'static str = "payable_entry_offsets";
    /// `supplier_payment` 集合名。
    const SUPPLIER_PAYMENTS: &'static str = "supplier_payments";
    /// `payment_allocation` 集合名。
    const PAYMENT_ALLOCATIONS: &'static str = "payment_allocations";
    /// `purchase_invoice_allocation` 集合名。
    const PURCHASE_INVOICE_ALLOCATIONS: &'static str = "purchase_invoice_allocations";

    /// 应付往来子账列表筛选条件类型（定义见 `repository::payable`）。
    type PayableAccountFilter;

    /// 供应商付款单列表筛选条件类型（定义见 `repository::payable`）。
    type SupplierPaymentFilter;

    /// 进项发票分配服务端分页筛选条件类型（定义见 `repository::payable`）。
    type PurchaseInvoiceAllocationFilter;

    /// 获取 `payable_account` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PayableAccountRepository<'_>`。
    fn payable_accounts(&self) -> PayableAccountRepository<'_>;

    /// 获取 `payable_entry` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PayableEntryRepository<'_>`。
    fn payable_entries(&self) -> PayableEntryRepository<'_>;

    /// 获取 `payable_entry_offset` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PayableEntryOffsetRepository<'_>`。
    fn payable_entry_offsets(&self) -> PayableEntryOffsetRepository<'_>;

    /// 获取 `supplier_payment` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `SupplierPaymentRepository<'_>`。
    fn supplier_payments(&self) -> SupplierPaymentRepository<'_>;

    /// 获取 `payment_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PaymentAllocationRepository<'_>`。
    fn payment_allocations(&self) -> PaymentAllocationRepository<'_>;

    /// 获取 `purchase_invoice_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseInvoiceAllocationRepository<'_>`。
    fn purchase_invoice_allocations(&self) -> PurchaseInvoiceAllocationRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `PayableRepository` 实例。
    fn payable(&self) -> PayableRepository<'_>;
}

impl PayableExt for Database {
    type PayableAccountFilter = PayableAccountFilter;
    type SupplierPaymentFilter = SupplierPaymentFilter;
    type PurchaseInvoiceAllocationFilter = PurchaseInvoiceAllocationFilter;

    fn payable_accounts(&self) -> PayableAccountRepository<'_> {
        PayableAccountRepository::new(self, Self::PAYABLE_ACCOUNTS)
    }

    fn payable_entries(&self) -> PayableEntryRepository<'_> {
        PayableEntryRepository::new(self, Self::PAYABLE_ENTRIES)
    }

    fn payable_entry_offsets(&self) -> PayableEntryOffsetRepository<'_> {
        PayableEntryOffsetRepository::new(self, Self::PAYABLE_ENTRY_OFFSETS)
    }

    fn supplier_payments(&self) -> SupplierPaymentRepository<'_> {
        SupplierPaymentRepository::new(self, Self::SUPPLIER_PAYMENTS)
    }

    fn payment_allocations(&self) -> PaymentAllocationRepository<'_> {
        PaymentAllocationRepository::new(self, Self::PAYMENT_ALLOCATIONS)
    }

    fn purchase_invoice_allocations(&self) -> PurchaseInvoiceAllocationRepository<'_> {
        PurchaseInvoiceAllocationRepository::new(self, Self::PURCHASE_INVOICE_ALLOCATIONS)
    }

    fn payable(&self) -> PayableRepository<'_> {
        PayableRepository::new(self)
    }
}
