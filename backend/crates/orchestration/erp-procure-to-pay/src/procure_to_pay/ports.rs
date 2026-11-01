//! 采购业务消费的明确应用能力，不持有其他编排的完整 Process。

use std::sync::Arc;

use erp_approval_runtime::ApprovalRuntimePort;
use erp_fulfillment_execution::fulfillment_execution::task::FulfillmentIdentityFactory;
use erp_procurement::{PurchaseAccess, PurchaseDataScopePort};
use erp_workflow::ports::FinanceResponsibilityPort;
use mongodb::Database;

/// 采购命令共享的范围、审批与任务责任能力，由应用实际提供者装配。
#[derive(Clone)]
pub struct PurchaseOrderPorts {
    pub(crate) task_identity: FulfillmentIdentityFactory,
    pub(crate) scope: Arc<dyn PurchaseDataScopePort>,
    pub(crate) approval: Arc<dyn ApprovalRuntimePort>,
    pub(crate) finance_responsibility: Arc<dyn FinanceResponsibilityPort>,
}

impl PurchaseOrderPorts {
    /// 绑定应用提供的现有领域能力，不查询或写入。
    /// # 参数
    /// `scope`、`approval` 和 `finance_responsibility` 分别为采购范围、审批和财务任务责任。
    /// `task_identity` 仅在新履约任务资格核验时创建独立身份。
    /// # 返回
    /// 返回可共享的采购命令能力。
    /// # 错误
    /// 不返回错误。
    pub fn new(
        scope: Arc<dyn PurchaseDataScopePort>,
        approval: Arc<dyn ApprovalRuntimePort>,
        finance_responsibility: Arc<dyn FinanceResponsibilityPort>,
        task_identity: FulfillmentIdentityFactory,
    ) -> Self {
        Self { scope, approval, finance_responsibility, task_identity }
    }

    pub(super) fn access(&self, db: Database) -> PurchaseAccess {
        PurchaseAccess::new(db, self.scope.clone())
    }
}
