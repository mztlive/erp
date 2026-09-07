//! 应付、供应商付款与来源单据的跨领域只读投影。
use erp_finance::dto::payable as dto;
use erp_finance::repository::PayableExt;
use mongodb::Database;
mod account;
mod display;
pub mod mapping;
mod payment;
/// 跨领域应付读取服务；批量装载与历史主数据缺失策略保持原合同。
pub struct PayableReadService {
    db: Database,
}
impl PayableReadService {
    /// 为同一个数据库创建应付读取服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
type PayableAccountFilter = <Database as PayableExt>::PayableAccountFilter;
type SupplierPaymentFilter = <Database as PayableExt>::SupplierPaymentFilter;
