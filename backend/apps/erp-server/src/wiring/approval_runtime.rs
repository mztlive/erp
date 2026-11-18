//! 共有审批步骤的应用装配。
use std::sync::Arc;

use erp_approval_runtime::{ApprovalRuntime, ApprovalRuntimePort};
use erp_identity::{AuthorizationService, SharedRbacService};
use mongodb::Database;

use super::workflow::{workflow_audit, workflow_auth};

/// 为业务 Process 注入审批运行时的窄合同：材料准备、定义绑定、启动与取消抄送。构造复用共享授权和 RBAC，资格及持久化错误在实际调用时返回。
pub fn approval_runtime(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> Arc<dyn ApprovalRuntimePort> {
    Arc::new(ApprovalRuntime::new(
        db.clone(),
        authorization.clone(),
        rbac.clone(),
        workflow_auth(db.clone(), rbac, authorization),
        workflow_audit(db),
    ))
}
