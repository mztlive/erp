use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_identity::AccessTarget;
use erp_permissions::supplier_settlement;
use erp_supply::service::supplier_settlement::SettlementAction;
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, Transactional};

use super::{SupplierSettlementProcess, SupplierSettlementStatementView, VoidSettlementRequest};
use crate::Result;

impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 作废尚未提交复核的结算草稿。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 作废请求（含期望版本与原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后结算单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `ConflictError` - 版本冲突
    pub async fn void_statement(
        &self,
        id: &str,
        req: VoidSettlementRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementView> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.void_statement_apply(&id, req, &actor, executor).await })
            })
            .await
    }

    /// 在入口事务内完成授权、准备、重放与持久化。
    async fn void_statement_apply(
        &self,
        id: &str,
        req: VoidSettlementRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatementView> {
        self.authorization
            .require(actor, &supplier_settlement::STATEMENT_VOID, AccessTarget::Operation, executor)
            .await?;
        self.domain()
            .access()
            .require_statement(actor, &supplier_settlement::STATEMENT_REFRESH, id, executor)
            .await?;
        let (mut statement, replayed) = self.domain().prepare_void(id, &req, actor, executor).await?;
        if replayed {
            return Ok(statement.into());
        }
        let audit = actor.clone().resource_log(
            "supplier_settlement.void",
            "supplier_settlement_statement",
            id.to_string(),
        )?;
        self.data_scope.require_action(actor, SettlementAction::Void, &statement.base.id, executor).await?;
        self.domain().persist_statement(&mut statement, executor).await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(statement.into())
    }
}
