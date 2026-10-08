//! 应付单域查询与事务内核销合同；外域任务和审计由流程协调。
use mongodb::Database;
mod account;
mod invoice;
pub mod purchase_change;
pub mod purchase_initial;
pub use invoice::{persist_purchase_invoice, prepare_purchase_invoice, prepare_purchase_invoice_allocations};
mod payment;
pub use account::prepare_payable_account;
pub use payment::{
    PaymentSettlement, PaymentSettlementFacts, finish_supplier_payment, settle_supplier_payment,
    settle_supplier_payment_with_facts,
};
/// 应付领域服务，仅读取与写入财务事实。
pub struct PayableService {
    db: Database,
}
impl PayableService {
    /// 创建应付财务服务。
    ///
    /// # 参数
    /// * `db` - 财务领域数据库。
    ///
    /// # 返回
    /// 返回未开始任何读写的服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

pub mod supplier_refund;

pub mod payment_reversal;

pub mod offset_batch;

pub mod supplier_settlement;
