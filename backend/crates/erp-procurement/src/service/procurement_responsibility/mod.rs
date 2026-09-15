//! 采购责任候选解析与本域规则写入；授权策略和审计事务由流程根负责。

mod resolver;
mod rules;

use mongodb::Database;
pub use resolver::{CandidateResolution, ResolutionInput, eligible_owner};

/// 采购责任领域服务，不持有身份或目录提供方。
pub struct ProcurementResponsibilityService {
    db: Database,
}

impl ProcurementResponsibilityService {
    /// 使用采购拥有仓储创建领域服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
