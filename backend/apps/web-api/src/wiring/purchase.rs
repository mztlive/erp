//! 采购命令和最终审批使用的应用合同装配。
use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_procure_to_pay::procure_to_pay::{PurchaseOrderFormalizationProcess, PurchaseOrderPorts};
use mongodb::Database;

use super::{MongoPurchaseDataScope, approval_runtime, finance_responsibility, fulfillment_identity_factory};

/// 为采购命令绑定当前范围、审批步骤和财务任务责任合同。
/// # 参数
/// * `db` - 采购及相关领域所在数据库。
/// * `rbac` - 当前共享授权服务。
/// # 返回
/// 返回普通采购命令与最终审批共同使用的合同。
/// # 错误
/// 不返回错误；资格与版本在原业务事务中重验。
pub fn purchase_ports(db: Database, rbac: SharedRbacService) -> Arc<PurchaseOrderPorts> {
    Arc::new(PurchaseOrderPorts::new(
        MongoPurchaseDataScope::shared(db.clone(), rbac.clone()),
        approval_runtime(db.clone(), rbac.clone()),
        finance_responsibility(db.clone(), rbac),
        fulfillment_identity_factory(db),
    ))
}

/// 为采购最终审批接入与普通命令一致的实际应用能力。
/// # 参数
/// * `db` - 采购及相关领域所在数据库。
/// * `rbac` - 当前共享授权服务。
/// # 返回
/// 返回完成合同注入的最终审批流程。
/// # 错误
/// 不返回错误；正式版本、财务和履约写入在原 Executor 内完成。
pub fn purchase_formalization_process(
    db: Database,
    rbac: SharedRbacService,
) -> PurchaseOrderFormalizationProcess {
    let ports = purchase_ports(db.clone(), rbac.clone());
    PurchaseOrderFormalizationProcess::new(db, rbac, ports)
}
