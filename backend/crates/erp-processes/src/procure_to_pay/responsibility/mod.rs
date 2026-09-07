//! 采购责任授权、规则维护审计事务与逐行解析预览。

mod adapter;
mod authorization;
mod resolver;
mod service;

use erp_identity::SharedRbacService;
use mongodb::Database;
pub use resolver::{AuthorizedResolutionLine, AuthorizedResolutionPlan, ResolutionInput};

/// 采购责任跨域用例根：持有共享授权服务并维护原策略版本提交栅栏。
pub struct ProcurementResponsibilityProcess {
    db: Database,
    rbac: SharedRbacService,
}

impl ProcurementResponsibilityProcess {
    /// 注入当前数据库和共享授权服务，不创建独立策略快照。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
}
