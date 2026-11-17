//! 集成服务的组合根装配。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_integration::{IntegrationAccess, IntegrationOpsService};
use mongodb::Database;

/// 装配复用完整统一授权服务的集成本域服务。
/// # 参数
/// db、authorization 为应用共享身份设施。
/// # 返回
/// 绑定真实授权能力的服务。
/// # 错误
/// 构造不执行 I/O。
pub fn scoped_integration_ops_service(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> IntegrationOpsService {
    IntegrationOpsService::new(db.clone(), IntegrationAccess::new(db, authorization))
}
