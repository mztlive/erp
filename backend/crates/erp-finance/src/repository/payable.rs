//! 域 D19 `payable` 仓储：payable_account、payable_entry、payable_entry_offset、
//! supplier_payment、payment_allocation、purchase_invoice_allocation。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；本模块只补充域特有查询、
//! **条件核销进度更新**（原子写入口，不超额核销，P2 计划 §5）与跨集合多步骤
//! 事务写入入口。集合名常量统一从 `indexes::payable` 导入。
//!
//! 正式事实集合（分录、抵销、分配）过账后不可更新或删除，**不提供软删除方法**；
//! `payable_account` 是稳定主表类，可软删除与恢复。`invoice` 由 D18 拥有，
//! 本域只通过 `ReceivableExt::invoices()` 在 P3 复用。
//!
//! 筛选/行类型定义在职责子模块，经本模块重新导出并由 `PayableExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use mongodb::Database;

use super::extensions::PayableExt;

mod account;
mod allocation;
mod command;
mod entry;
mod payment;

pub use account::{
    PayableAccountFilter, PayableAccountInvoicingExt, PayableAccountRepositoryExt, PayableAccountRow,
    PayableAccountSettlementExt,
};
pub use allocation::{
    PaymentAllocationRepositoryExt, PurchaseInvoiceAllocationFilter, PurchaseInvoiceAllocationRepositoryExt,
};
pub use entry::{PayableEntryOffsetRepositoryExt, PayableEntryRepositoryExt};
pub use payment::{SupplierPaymentFilter, SupplierPaymentRepositoryExt, SupplierPaymentRow};

/// `payable_entry` 集合名（单一来源：`PayableExt` 关联常量）。
const PAYABLE_ENTRIES: &str = <mongodb::Database as PayableExt>::PAYABLE_ENTRIES;

/// D19 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `PayableExt::payable()` 访问。
pub struct PayableRepository<'a> {
    db: &'a Database,
}

impl<'a> PayableRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}

pub(super) use super::sort_doc_with_id as sort_doc;
