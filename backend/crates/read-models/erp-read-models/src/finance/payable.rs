//! 应付、供应商付款与来源单据的跨领域只读投影。
use erp_finance::dto::payable as dto;
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
    /// 装配供应商应付与付款时，为同一个数据库创建应付读取服务。
    ///
    /// # 返回
    /// 返回应付读取服务；构造不执行 I/O。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
