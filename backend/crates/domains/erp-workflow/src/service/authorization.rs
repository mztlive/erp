//! 工作流功能资格仅使用 identity 的公开批量入口。
use erp_identity::{AuthorizationRegistry, AuthorizationService, SharedRbacService};
use mongodb::Database;

/// 装配功能资格服务；对象授权由任务及业务来源的固定登记执行。
/// # 参数
/// db、identity 为工作流当前使用的身份基础设施。
/// # 返回
/// 不执行 I/O 的统一功能资格入口。
/// # 错误
/// 不返回错误。
pub(crate) fn features(db: &Database, identity: &SharedRbacService) -> AuthorizationService {
    AuthorizationService::new(db.clone(), identity.clone(), AuthorizationRegistry::default())
}
