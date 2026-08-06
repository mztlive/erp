//! 域 D18 `receivable` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS` 等值。
//!
//! `invoice` 由 D18 拥有（domains.md §3 唯一跨批次共享聚合），D19 在 P3 通过
//! `invoices()` 访问器复用，禁止复制发票实体或另建访问路径。

use entities::receivable::{
    CustomerReceipt, Invoice, ReceiptAllocation, ReceivableAccount, ReceivableEntry, ReceivableEntryOffset,
    ReceivableFundsReview, SalesInvoiceAllocation,
};
use mongodb::Database;

use super::super::receivable::{
    CustomerReceiptFilter, InvoiceFilter, ReceivableAccountFilter, ReceivableRepository,
};
use crate::Repository;

/// 域 D18 仓储访问器。
pub trait ReceivableExt {
    /// `receivable_account` 集合名。
    const RECEIVABLE_ACCOUNTS: &'static str = "receivable_accounts";
    /// `receivable_entry` 集合名。
    const RECEIVABLE_ENTRIES: &'static str = "receivable_entries";
    /// `receivable_funds_review` 集合名。
    const RECEIVABLE_FUNDS_REVIEWS: &'static str = "receivable_funds_reviews";
    /// `receivable_entry_offset` 集合名。
    const RECEIVABLE_ENTRY_OFFSETS: &'static str = "receivable_entry_offsets";
    /// `customer_receipt` 集合名。
    const CUSTOMER_RECEIPTS: &'static str = "customer_receipts";
    /// `receipt_allocation` 集合名。
    const RECEIPT_ALLOCATIONS: &'static str = "receipt_allocations";
    /// `invoice` 集合名。
    const INVOICES: &'static str = "invoices";
    /// `sales_invoice_allocation` 集合名。
    const SALES_INVOICE_ALLOCATIONS: &'static str = "sales_invoice_allocations";

    /// 应收往来子账列表筛选条件类型（定义见 `repository::receivable`）。
    type ReceivableAccountFilter;

    /// 客户回款单列表筛选条件类型（定义见 `repository::receivable`）。
    type CustomerReceiptFilter;

    /// 发票列表筛选条件类型（定义见 `repository::receivable`）。
    type InvoiceFilter;

    /// 获取 `receivable_account` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::ReceivableAccount>`。
    fn receivable_accounts(&self) -> Repository<'_, ReceivableAccount>;

    /// 获取 `receivable_entry` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::ReceivableEntry>`。
    fn receivable_entries(&self) -> Repository<'_, ReceivableEntry>;

    /// 获取 `receivable_funds_review` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::ReceivableFundsReview>`。
    fn receivable_funds_reviews(&self) -> Repository<'_, ReceivableFundsReview>;

    /// 获取 `receivable_entry_offset` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::ReceivableEntryOffset>`。
    fn receivable_entry_offsets(&self) -> Repository<'_, ReceivableEntryOffset>;

    /// 获取 `customer_receipt` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::CustomerReceipt>`。
    fn customer_receipts(&self) -> Repository<'_, CustomerReceipt>;

    /// 获取 `receipt_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::ReceiptAllocation>`。
    fn receipt_allocations(&self) -> Repository<'_, ReceiptAllocation>;

    /// 获取 `invoice` 集合的 Repository（D19 通过本访问器复用）。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::Invoice>`。
    fn invoices(&self) -> Repository<'_, Invoice>;

    /// 获取 `sales_invoice_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::receivable::SalesInvoiceAllocation>`。
    fn sales_invoice_allocations(&self) -> Repository<'_, SalesInvoiceAllocation>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `ReceivableRepository` 实例。
    fn receivable(&self) -> ReceivableRepository<'_>;
}

impl ReceivableExt for Database {
    type ReceivableAccountFilter = ReceivableAccountFilter;
    type CustomerReceiptFilter = CustomerReceiptFilter;
    type InvoiceFilter = InvoiceFilter;

    fn receivable_accounts(&self) -> Repository<'_, ReceivableAccount> {
        Repository::new(self, Self::RECEIVABLE_ACCOUNTS)
    }

    fn receivable_entries(&self) -> Repository<'_, ReceivableEntry> {
        Repository::new(self, Self::RECEIVABLE_ENTRIES)
    }

    fn receivable_funds_reviews(&self) -> Repository<'_, ReceivableFundsReview> {
        Repository::new(self, Self::RECEIVABLE_FUNDS_REVIEWS)
    }

    fn receivable_entry_offsets(&self) -> Repository<'_, ReceivableEntryOffset> {
        Repository::new(self, Self::RECEIVABLE_ENTRY_OFFSETS)
    }

    fn customer_receipts(&self) -> Repository<'_, CustomerReceipt> {
        Repository::new(self, Self::CUSTOMER_RECEIPTS)
    }

    fn receipt_allocations(&self) -> Repository<'_, ReceiptAllocation> {
        Repository::new(self, Self::RECEIPT_ALLOCATIONS)
    }

    fn invoices(&self) -> Repository<'_, Invoice> {
        Repository::new(self, Self::INVOICES)
    }

    fn sales_invoice_allocations(&self) -> Repository<'_, SalesInvoiceAllocation> {
        Repository::new(self, Self::SALES_INVOICE_ALLOCATIONS)
    }

    fn receivable(&self) -> ReceivableRepository<'_> {
        ReceivableRepository::new(self)
    }
}
