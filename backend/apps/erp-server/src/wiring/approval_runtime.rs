//! 共有审批步骤的应用装配。
use std::sync::Arc;

use erp_approval_runtime::{ApprovalRuntime, ApprovalRuntimePort};
use erp_identity::SharedRbacService;
use mongodb::Database;

use super::identity::shared_rbac_service;
use super::workflow::{workflow_audit, workflow_auth};

/// 为当前数据库与授权源装配共有审批执行合同。
/// # 参数
/// * `db` - 审批材料、工作流与业务事实所在数据库。
/// * `rbac` - 当前应用共享的授权服务。
/// # 返回
/// 返回只公开材料、绑定、启动与取消拷贝窄步骤的动态合同。
/// # 错误
/// 不返回错误；业务资格与持久化错误由实际调用点返回。
pub fn approval_runtime(db: Database, rbac: SharedRbacService) -> Arc<dyn ApprovalRuntimePort> {
    let material_db = db.clone();
    Arc::new(ApprovalRuntime::new(
        workflow_auth(db.clone(), rbac),
        workflow_audit(db),
        Arc::new(move || workflow_auth(material_db.clone(), shared_rbac_service(material_db.clone()))),
    ))
}
