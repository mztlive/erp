//! 结算命令前的独立详情资格检查。
use application_core::AuditActor;
use erp_permissions::supplier_settlement;
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::ReadExecutor;

use super::SupplierSettlementProcess;
use crate::Result;
impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 按详情权限确认当前账号能看到指定结算单，供只需要访问校验的入口使用，不加载整单视图；不可见时按 `NotFound` 处理。
    pub async fn require_detail(&self, actor: &AuditActor, id: &str) -> Result<()> {
        self.domain()
            .access()
            .require_statement(
                actor,
                &supplier_settlement::STATEMENT_DETAIL,
                id,
                &mut ReadExecutor::default(),
            )
            .await?;
        Ok(())
    }
}
