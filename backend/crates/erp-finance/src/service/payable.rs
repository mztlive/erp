//! 应付单域查询与事务内核销合同；外域任务和审计由流程协调。
use mongodb::Database;
mod account;
mod invoice;
pub mod purchase_change;
pub mod purchase_initial;
pub use invoice::{
    persist_purchase_invoice_in_transaction, prepare_purchase_invoice,
    prepare_purchase_invoice_allocations_in_transaction,
};
mod payment;
pub use account::prepare_payable_account;
pub use payment::{
    finish_supplier_payment_in_transaction, settle_supplier_payment_in_transaction, PaymentSettlement,
};
/// 应付领域服务，仅读取与写入财务事实。
pub struct PayableService {
    db: Database,
}
impl PayableService {
    /// 创建应付财务服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

pub mod supplier_refund;

pub mod payment_reversal;

pub mod offset_batch;
