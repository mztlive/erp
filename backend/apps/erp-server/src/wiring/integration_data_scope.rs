//! 集成服务的组合根装配。
use erp_identity::SharedRbacService;
use erp_integration::{IntegrationAccess, IntegrationOpsService};
use mongodb::Database;

/// 装配已登记完整授权的集成本域服务。
/// # 参数
/// db、rbac 为应用共享身份设施。
/// # 返回
/// 绑定真实授权能力的服务。
/// # 错误
/// 构造不执行 I/O。
pub fn scoped_integration_ops_service(db: Database, rbac: SharedRbacService) -> IntegrationOpsService {
    IntegrationOpsService::new(db.clone(), IntegrationAccess::new(db, rbac))
}
