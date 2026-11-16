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
        self.data_scope.require_action(actor, SettlementAction::Void, id, executor).await?;
        self.authorization
            .require(actor, &supplier_settlement::STATEMENT_VOID, AccessTarget::Operation, executor)
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::Error;

    #[tokio::test]
    async fn void_replay_cannot_reach_preparation_without_current_command_access() {
        let process = super::super::tests::unwired_process().await;
        let actor = AuditActor::new("preparer".into(), "preparer-login".into(), AccountKind::Supplier);
        let result = process
            .void_statement_apply(
                "statement-1",
                VoidSettlementRequest { version: 1, reason: "重新结算".into() },
                &actor,
                &mut NoTransaction,
            )
            .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "供应链授权未接线"));
    }
}
