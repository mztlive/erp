//! 结算命令前的独立详情资格检查。
use application_core::AuditActor;
use erp_permissions::supplier_settlement;
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::ReadExecutor;

use super::SupplierSettlementProcess;
use crate::Result;
impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 按详情动作重验结算单可见性，不返回整单视图。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 对象在详情范围内时返回 `Ok(())`。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound。
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
