//! 仍由流程自行构造 RBAC 时的组合辅助。

use erp_identity::SharedRbacService;
use mongodb::Database;

use super::identity_audit::MongoIdentityAudit;

/// 用身份审计 adapter 组装流程可共用的 RBAC 服务。
///
/// # 参数
/// * `db` - 身份与审计集合所在数据库。
///
/// # 返回
/// 返回经 `erp-audit` 持久化身份审计的共享 RBAC 服务。
///
/// # 错误
/// 不返回错误。
pub fn shared_rbac_service(db: Database) -> SharedRbacService {
    let audit = MongoIdentityAudit::shared(db.clone());
    erp_identity::shared_rbac_service(db, audit)
}
