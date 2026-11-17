//! 采购命令和最终审批使用的应用合同装配。
use std::sync::Arc;

use erp_identity::{AuthorizationService, SharedRbacService};
use erp_procure_to_pay::procure_to_pay::{PurchaseOrderFormalizationProcess, PurchaseOrderPorts};
use erp_procurement::{PurchaseAccess, PurchaseDataScope};
use mongodb::Database;

use super::{approval_runtime, finance_responsibility};

/// 为采购命令绑定共享统一授权、审批步骤和财务任务责任合同。
/// # 参数
/// db 为业务数据库，rbac 为既有审批设施，authorization 为组合根共享统一授权服务。
/// # 返回
/// 返回普通采购命令与最终审批共同使用的合同。
/// # 错误
/// 无。
pub fn purchase_ports(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> Arc<PurchaseOrderPorts> {
    Arc::new(PurchaseOrderPorts::new(
        PurchaseDataScope::shared(db.clone(), authorization.clone()),
        approval_runtime(db.clone(), rbac.clone(), authorization.clone()),
        finance_responsibility(db, rbac, authorization.clone()),
        authorization,
    ))
}

/// 为采购最终审批注入普通命令使用的同一个共享统一授权服务。
/// # 参数
/// db 为业务数据库，rbac 为既有审批设施，authorization 为组合根共享统一授权服务。
/// # 返回
/// 返回完成合同注入的最终审批流程。
/// # 错误
/// 无。
pub fn purchase_formalization_process(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> PurchaseOrderFormalizationProcess {
    let ports = purchase_ports(db.clone(), rbac, authorization.clone());
    PurchaseOrderFormalizationProcess::new(db, authorization, ports)
}

/// 装配先授权记录再读取采购单的访问器。
/// # 参数
/// db 为原业务库，authorization 为组合根共享统一授权服务。
/// # 返回
/// 返回不保存授权缓存的采购访问器。
/// # 错误
/// 无。
pub fn purchase_access(db: Database, authorization: Arc<AuthorizationService>) -> PurchaseAccess {
    PurchaseAccess::new(db.clone(), PurchaseDataScope::shared(db, authorization))
}
