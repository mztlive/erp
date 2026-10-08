//! 无正式任务对账的任务关联闸门、事务与回执。
use application_core::AuditActor;
use erp_identity::{Permission, subject};
use erp_integration::dto::{
    DirectReconciliationCommand, DirectReconciliationResult, DirectReconciliationStatus,
    PreparedDirectDecisionTarget,
};
use erp_integration::entity::integration_ops::{DirectReceiptResult, IntegrationCommandIdentity};
use erp_integration::service::task_decision::DirectFact;
use erp_integration::service::task_decision::direct::execute_direct_decision;
use erp_workflow::WorkItemExt;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use super::super::IntegrationResolutionProcess;
use super::guard::command_identity;
use super::{DIRECT_DECISION_AUDIT, store_receipt};
use crate::{Error, Result};

/// 无任务直接决定的真实 provider；关联闸门属于流程、本域写入属于 integration。
struct DirectCommand<'a> {
    db: &'a Database,
    evidence: &'a dyn erp_integration::ports::evidence::IntegrationEvidenceAuthority,
    prepared: &'a PreparedDirectDecisionTarget,
    command: &'a DirectReconciliationCommand,
    actor: &'a AuditActor,
    receipt: &'a IntegrationCommandIdentity,
}

#[async_trait::async_trait]
impl super::execution::DirectCommandPort for DirectCommand<'_> {
    type Fact = DirectFact;
    type Output = DirectReconciliationResult;

    async fn ensure_no_task(&mut self, executor: &mut dyn Executor) -> Result<()> {
        ensure_no_work_item(self.db, &self.prepared.difference_id, executor).await
    }

    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact> {
        Ok(execute_direct_decision(
            self.db,
            self.evidence,
            self.prepared,
            self.command,
            self.receipt.receipt_id(),
            self.actor.id(),
            executor,
        )
        .await?)
    }

    async fn receipt(&mut self, fact: &Self::Fact, executor: &mut dyn Executor) -> Result<()> {
        store_direct_receipt(self.db, self.actor, self.receipt, fact, executor).await
    }

    fn result(&mut self, fact: Self::Fact) -> Self::Output {
        direct_result(self.command, self.receipt.receipt_id(), fact)
    }
}

impl IntegrationResolutionProcess {
    /// 对未关联任何正式任务的差异提交 decision-only 命令。
    ///
    /// # 参数
    /// * `path_id` - 路径中的差异 ID，必须与命令内身份一致。
    /// * `command` - 直接决定命令。
    /// * `actor` - 已通过鉴权的审计操作人。
    ///
    /// # 返回
    /// 本次决定或同载荷重放的结果。
    ///
    /// # 错误
    /// 请求非法、路径身份不一致、当前账号无权，或差异版本、任务关联、终态证据或幂等校验失败时返回错误。
    pub async fn decide_difference(
        &self,
        path_id: &str,
        command: DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<DirectReconciliationResult> {
        command.validate()?;
        self.ensure_direct_permission(actor).await?;
        if command.difference_id != path_id {
            return Err(Error::ValidationError("路径差异 ID 与命令不一致".to_string()));
        }
        let receipt = command_identity(
            actor.id(),
            DIRECT_DECISION_AUDIT,
            "reconciliation_difference",
            path_id,
            &command.idempotency_key,
            &command,
        )?;
        super::execution::execute_with_receipt(
            || self.replay_direct_decision(&receipt, &command, actor),
            || self.transact_direct_decision(command.clone(), actor.clone(), receipt.clone()),
        )
        .await
    }

    /// 独立决定与重放统一检验当前静态权限，任务关联由各自读取闸门拒绝。
    async fn ensure_direct_permission(&self, actor: &AuditActor) -> Result<()> {
        let rbac = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let permission = Permission::parse("reconciliation_difference:decide")?;
        if !rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await? {
            return Err(Error::Forbidden("当前账号已不具备对账差异决定权限".into()));
        }
        Ok(())
    }

    async fn transact_direct_decision(
        &self,
        command: DirectReconciliationCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<DirectReconciliationResult> {
        let attempt_context = super::audit_context(&actor, &receipt)?;
        let evidence = std::sync::Arc::clone(&self.evidence);
        let result = self
            .run_audited(move |db, executor| {
                Box::pin(async move {
                    let prepared = PreparedDirectDecisionTarget::try_from(&command)?;
                    super::execution::run_direct(
                        &mut DirectCommand {
                            db,
                            evidence: evidence.as_ref(),
                            prepared: &prepared,
                            command: &command,
                            actor: &actor,
                            receipt: &receipt,
                        },
                        executor,
                    )
                    .await
                })
            })
            .await;
        crate::audit::finish_attempt(
            result,
            &attempt_context,
            &crate::audit::MongoAuditAttemptSink::new(&self.db),
        )
        .await
    }

    async fn replay_direct_decision(
        &self,
        receipt: &IntegrationCommandIdentity,
        command: &DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<Option<DirectReconciliationResult>> {
        let Some(message) = self.replay_receipt::<DirectReceiptResult>(receipt, actor).await? else {
            return Ok(None);
        };
        self.ensure_direct_permission(actor).await?;
        ensure_no_work_item(&self.db, &command.difference_id, &mut persistence_core::NoTransaction).await?;
        Ok(Some(DirectReconciliationResult {
            difference_id: command.difference_id.clone(),
            operation_id: command.operation_id.clone(),
            resolution_record_id: receipt.receipt_id().to_string(),
            resulting_status: message.resulting_status,
            is_terminal: message.is_terminal,
            outcome: message.outcome,
            business_result_reference: message.business_result_reference,
        }))
    }
}

async fn ensure_no_work_item(db: &Database, difference_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let items = db.work_items().find_unique_for_reconciliation_difference(difference_id, executor).await?;
    if items.is_empty() {
        return Ok(());
    }
    Err(Error::ConflictError("差异已关联正式任务，必须通过 W29 任务强命令处理".to_string()))
}

fn direct_result(
    command: &DirectReconciliationCommand,
    receipt_id: &str,
    fact: DirectFact,
) -> DirectReconciliationResult {
    let is_terminal = matches!(
        fact.resulting_status,
        DirectReconciliationStatus::ConfirmedNoError | DirectReconciliationStatus::ConfirmedValidDifference
    );
    DirectReconciliationResult {
        difference_id: command.difference_id.clone(),
        operation_id: command.operation_id.clone(),
        resolution_record_id: receipt_id.to_string(),
        resulting_status: fact.resulting_status,
        is_terminal,
        outcome: fact.outcome,
        business_result_reference: fact.business_result_reference,
    }
}

async fn store_direct_receipt(
    db: &Database,
    actor: &AuditActor,
    receipt: &IntegrationCommandIdentity,
    fact: &DirectFact,
    executor: &mut dyn Executor,
) -> Result<()> {
    store_receipt(
        db,
        actor,
        receipt,
        DirectReceiptResult {
            resulting_status: fact.resulting_status,
            is_terminal: fact.resulting_status == DirectReconciliationStatus::ConfirmedNoError
                || fact.resulting_status == DirectReconciliationStatus::ConfirmedValidDifference,
            outcome: fact.outcome,
            business_result_reference: fact.business_result_reference.clone(),
        },
        executor,
    )
    .await
}
