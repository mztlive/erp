//! 本编排功能资格入口；对象授权由业务登记单独执行。
use erp_identity::{AuthorizationRegistry, AuthorizationService, SharedRbacService};
use mongodb::Database;
pub(crate) fn features(db: Database, rbac: SharedRbacService) -> AuthorizationService {
    AuthorizationService::new(db, rbac, AuthorizationRegistry::default())
}
