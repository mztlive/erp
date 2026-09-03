//! 采购责任规则管理、确定性解析与销售采购任务规划。

mod dto;
mod resolver;
mod rule_list;
mod service;

use crate::iam::SharedRbacService;
use mongodb::Database;

pub use dto::{
    CreateProcurementResponsibilityRuleRequest, ProcurementResponsibilityResolutionView,
    ProcurementResponsibilityResolveLineRequest, ProcurementResponsibilityResolveLineView,
    ProcurementResponsibilityResolveRequest, ProcurementResponsibilityResolveView,
    ProcurementResponsibilityRuleListParams, ProcurementResponsibilityRulePageView,
    ProcurementResponsibilityRuleView, UpdateProcurementResponsibilityRuleRequest,
};
pub(crate) use resolver::{AuthorizedResolutionPlan, ResolutionInput};

/// 采购责任服务。
pub struct ProcurementResponsibilityService {
    db: Database,
    rbac: SharedRbacService,
}

impl ProcurementResponsibilityService {
    /// 创建采购责任服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 共享 Casbin 授权服务
    ///
    /// # 返回
    /// 返回可用于规则维护、预览和销售形式化的服务。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
}
