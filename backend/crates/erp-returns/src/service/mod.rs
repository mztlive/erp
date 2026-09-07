//! 退货与资金纠错的本域规则、构造和事务内持久化。
pub mod approval;
pub mod customer_refund;
pub mod payment_reversal;
pub mod purchase_return;
pub mod receipt_reversal;
pub mod sales_return;
pub mod shared;
pub mod supplier_refund;
pub mod version_conflict;
use mongodb::Database;
/// 仅持有本域仓储访问，不创建跨域业务事务。
pub struct ReturnsService {
    db: Database,
}
impl ReturnsService {
    /// 使用调用者数据库句柄构造本域服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
