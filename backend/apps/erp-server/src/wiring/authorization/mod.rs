//! HTTP 功能资格装配；对象及查询授权由所属领域登记。
use erp_identity::{AuthorizationRegistry, AuthorizationService, SharedRbacService};
use mongodb::Database;

/// 创建仅供功能资格读取的身份域服务。
/// # 参数
/// db、rbac 为应用共享身份设施。
/// # 返回
/// 统一功能资格服务。
/// # 错误
/// 构造不执行 I/O。
pub fn authorization_service(db: Database, rbac: SharedRbacService) -> AuthorizationService {
    AuthorizationService::new(db, rbac, AuthorizationRegistry::default())
}
