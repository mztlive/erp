//! 财务过账复用进程共享授权及原业务责任分配能力。
use std::sync::Arc;

use erp_finance_posting::FinancePostingPorts;
use erp_identity::{AuthorizationService, SharedRbacService};
use mongodb::Database;

use super::finance_responsibility::finance_tasks;
use super::identity::shared_rbac_service;
use super::{approval_runtime, funds_access};

/// 注入财务命令所需完整共享授权及工作流能力。
/// # 参数
/// db、rbac 为共享基础设施，authorization 为启动时完成绑定的授权服务。
/// # 返回
/// 不执行 I/O 的财务业务能力集合。
/// # 错误
/// 构造不返回错误；业务检查使用原执行器。
pub fn finance_posting_ports(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> FinancePostingPorts {
    FinancePostingPorts::new(
        rbac.clone(),
        authorization.clone(),
        approval_runtime(db.clone(), rbac.clone(), authorization.clone()),
        funds_access(db.clone(), authorization.clone()),
        finance_tasks(db, rbac, authorization),
    )
}

/// 为客户往来命令连接共享授权及身份基础设施。
/// # 参数
/// db 为原业务数据库，authorization 为进程共享统一授权服务。
/// # 返回
/// 与其他命令复用授权绑定的财务能力集合。
/// # 错误
/// 构造不执行 I/O。
pub fn receivable_posting_ports(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> FinancePostingPorts {
    finance_posting_ports(db.clone(), shared_rbac_service(db), authorization)
}
