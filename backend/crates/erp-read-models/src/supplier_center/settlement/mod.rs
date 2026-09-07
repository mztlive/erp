//! 供应商结算跨域详情的唯一读模型。
pub mod dto;
mod query;
use mongodb::Database;
/// 供应链快照与正式财务复核任务的读取入口。
pub struct SupplierSettlementReadService {
    db: Database,
}
impl SupplierSettlementReadService {
    /// 使用原数据库构造结算读模型。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[cfg(test)]
mod tests;
