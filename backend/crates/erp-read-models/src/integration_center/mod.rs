//! 集成异常与差异详情：组合本域事实、正式责任与权威证据。
mod error_task;
mod reconciliation_difference;
use erp_integration::ports::evidence::IntegrationEvidenceAuthority;
use mongodb::Database;
use std::sync::Arc;
/// 只读详情服务；权威证据实现与写入口由组合根共享。
pub struct IntegrationCenterReadService {
    db: Database,
    evidence: Arc<dyn IntegrationEvidenceAuthority>,
}
impl IntegrationCenterReadService {
    /// 绑定数据库和明确注入的权威证据端口。
    pub fn new(db: Database, evidence: Arc<dyn IntegrationEvidenceAuthority>) -> Self {
        Self { db, evidence }
    }
}
