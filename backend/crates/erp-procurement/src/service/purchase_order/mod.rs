//! 采购订单单域构造、状态校验与调用方事务内持久化。
use mongodb::Database;
pub mod allocation_maintenance;
pub mod cancel_approval;
pub mod change;
pub mod coverage;
pub mod create_submit;
pub mod creation_basis;
pub mod draft_edit;
pub mod formalization;
pub mod lifecycle;
pub mod line_input;
pub mod shared;
pub mod start_approval;
pub mod submission;
pub mod view_mapping;
pub mod void_order;
/// 仅拥有采购领域数据和持久化的应用服务。
pub struct PurchaseOrderService {
    pub(crate) db: Database,
}
impl PurchaseOrderService {
    /// 使用给定数据库构造采购单域服务；事务由调用方执行器明确传入。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
