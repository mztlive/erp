//! 仍由流程自行构造 RBAC 时的组合辅助。

use erp_identity::SharedRbacService;
use mongodb::Database;

use super::identity_audit::MongoIdentityAudit;

/// 为 CLI 的管理员用例装配共享 RBAC 服务，将身份审计交给 `erp-audit` 写入同一数据库。
pub fn shared_rbac_service(db: Database) -> SharedRbacService {
    let audit = MongoIdentityAudit::shared(db.clone());
    erp_identity::shared_rbac_service(db, audit)
}
