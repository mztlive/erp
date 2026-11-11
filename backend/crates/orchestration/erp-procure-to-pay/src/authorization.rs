//! 本编排的功能资格入口；对象资格由业务登记另行证明。
use erp_identity::{AuthorizationRegistry, AuthorizationService, SharedRbacService};
use mongodb::Database;

pub(crate) fn features(db: Database, rbac: SharedRbacService) -> AuthorizationService {
    AuthorizationService::new(db, rbac, AuthorizationRegistry::default())
}
