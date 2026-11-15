//! 逆向业务消费的应用能力；构造根明确注入身份、审批与采购访问提供者。

use std::ops::Deref;
use std::sync::Arc;

use erp_approval_runtime::ApprovalRuntimePort;
use erp_identity::AuthorizationService;
use erp_procurement::{PurchaseAccess, PurchaseDataScopePort};
use erp_workflow::ports::FinanceResponsibilityPort;

/// 逆向命令需要的身份、审批运行、采购范围和财务任务责任能力。
#[derive(Clone)]
pub struct ReverseFlowPorts {
    pub(crate) identity: Arc<AuthorizationService>,
    pub(crate) runtime: Arc<dyn ApprovalRuntimePort>,
    pub(crate) purchase: PurchaseAccess,
    pub(crate) purchase_scope: Arc<dyn PurchaseDataScopePort>,
    pub(crate) finance_responsibility: Arc<dyn FinanceResponsibilityPort>,
}

impl ReverseFlowPorts {
    /// 注入应用已经装配的提供者，不创建权限缓存或执行I/O。
    /// # 参数
    /// 各参数分别为当前身份、审批运行、采购访问/范围和财务责任提供者。
    /// # 返回
    /// 返回可跨事务准备步骤克隆的本库能力集合。
    /// # 错误
    /// 构造时不返回错误，业务校验仍在原调用点执行。
    pub fn new(
        identity: Arc<AuthorizationService>,
        runtime: Arc<dyn ApprovalRuntimePort>,
        purchase: PurchaseAccess,
        purchase_scope: Arc<dyn PurchaseDataScopePort>,
        finance_responsibility: Arc<dyn FinanceResponsibilityPort>,
    ) -> Self {
        Self { identity, runtime, purchase, purchase_scope, finance_responsibility }
    }
}

impl Deref for ReverseFlowPorts {
    type Target = Arc<AuthorizationService>;

    fn deref(&self) -> &Self::Target {
        &self.identity
    }
}
