//! 供应商结算跨域详情的唯一读模型。
use std::sync::Arc;

use erp_identity::AuthorizationService;
mod display;
pub mod display_dto;
mod display_facts;
mod display_mapping;
pub mod dto;
mod statement_result;
pub use statement_result::SettlementStatementResult;
mod list;
mod query;
mod review_access;
use mongodb::Database;
pub use review_access::require_reviewer;
/// 供应链快照与正式财务复核任务的读取入口。
pub struct SupplierSettlementReadService {
    db: Database,
    identity: Arc<AuthorizationService>,
}
impl SupplierSettlementReadService {
    /// 装配供应商结算时，使用原数据库构造结算读模型。
    ///
    /// # 参数
    /// * `identity` - 共享身份设施。
    ///
    /// # 返回
    /// 返回结算读模型。构造不执行查询。
    pub fn new(db: Database, identity: Arc<AuthorizationService>) -> Self {
        Self { db, identity }
    }
}
