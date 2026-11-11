//! 结算编排的功能资格入口；完整对象和来源资格另行由登记政策证明。
use erp_identity::{AuthorizationRegistry, AuthorizationService, SharedRbacService};
use mongodb::Database;
pub(crate) fn features(db: Database, rbac: SharedRbacService) -> AuthorizationService {
    AuthorizationService::new(db, rbac, AuthorizationRegistry::new(vec![]).expect("空功能目录合法"))
}
