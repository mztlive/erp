//! 退货、退款与冲正的详情和分页读模型。
mod approval;
mod customer_refund;
mod customer_refund_list;
pub mod dto;
mod payment_reversal;
mod purchase_return;
mod receipt_reversal;
#[cfg(test)]
mod repository;
mod sales_return;
mod supplier_refund;
use mongodb::Database;
/// 组合逆向本域事实与只读审批摘要的查询入口。
pub struct ReturnsReadService {
    db: Database,
}
impl ReturnsReadService {
    /// 使用组合根提供的数据库读取退货与审批事实。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
