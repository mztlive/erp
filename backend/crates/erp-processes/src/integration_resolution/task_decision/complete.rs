//! 任务完成的回执、正式权限与跨域事务。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_integration::dto::{
    IntegrationTaskCompletionCommand, IntegrationTaskCompletionResult, IntegrationWorkItemStatus,
    PreparedWorkItemTarget,
};
use erp_integration::entity::integration_ops::{CompletionReceiptResult, IntegrationCommandIdentity};
use erp_integration::service::task_decision::complete::complete_domain_item;
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;

use super::super::IntegrationResolutionProcess;
use super::guard::{command_identity, load_bound_work_item};
use super::{TASK_COMPLETION_AUDIT, store_receipt};
use crate::Result;
use crate::adapters::workflow::work_item_service;

/// 正式任务完成步骤；本域终态写入先于 WorkItem 完成与回执。
struct CompletionCommand<'a> {
    db: &'a Database,
    evidence: &'a dyn erp_integration::ports::evidence::IntegrationEvidenceAuthority,
    rbac: &'a erp_identity::SharedRbacService,
    prepared: &'a PreparedWorkItemTarget,
    command: &'a IntegrationTaskCompletionCommand,
    actor: &'a AuditActor,
    receipt: &'a IntegrationCommandIdentity,
}

#[async_trait::async_trait]
impl super::execution::TaskCommandPort for CompletionCommand<'_> {
    type Item = erp_workflow::entity::work_item::WorkItem;
    type Fact = erp_integration::service::task_decision::complete::TerminalFact;
    type Output = IntegrationTaskCompletionResult;

    async fn load_bound(&mut self, executor: &mut dyn Executor) -> Result<Self::Item> {
        let action = self.command.as_non_terminal_action();
        load_bound_work_item(self.db, self.prepared, &action, self.actor.id(), executor).await
    }

    async fn authorize(&mut self, item: &Self::Item, executor: &mut dyn Executor) -> Result<()> {
        work_item_service(self.db.clone(), self.rbac.clone())
            .ensure_domain_decision_access(self.actor, item, executor)
            .await?;
        Ok(())
    }

    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact> {
        Ok(complete_domain_item(
            self.db,
            self.evidence,
            self.command,
            self.receipt.receipt_id(),
            self.actor.id(),
            executor,
        )
        .await?)
    }

    fn transition(&mut self, item: &mut Self::Item, fact: &Self::Fact) -> Result<()> {
        item.subject_version = fact.next_subject_version.clone();
        item.complete_by_domain_command(self.actor.id(), Instant::now())?;
        Ok(())
    }

    async fn persist_task(&mut self, item: &mut Self::Item, executor: &mut dyn Executor) -> Result<()> {
        self.db.work_items().update(item, executor).await?;
        Ok(())
    }

    fn result(&mut self, fact: &Self::Fact) -> Result<Self::Output> {
        Ok(completion_result(self.command, self.receipt.receipt_id(), fact.reference.clone()))
    }

    async fn receipt(&mut self, fact: &Self::Fact, executor: &mut dyn Executor) -> Result<()> {
        store_completion_receipt(self.db, self.actor, self.receipt, &fact.reference, executor).await
    }
}

impl IntegrationResolutionProcess {
    /// 执行 W29 任务完成强命令。
    ///
    /// # 参数
    /// * `command` - 任务完成强命令。
    /// * `actor` - 已通过鉴权的审计操作人。
    ///
    /// # 返回
    /// 本次完成或同载荷重放的结果。
    ///
    /// # 错误
    /// 请求非法、命令无法形成幂等指纹、无法由当前权威事实验证终态，或责任/版本/幂等校验失败时返回错误。
    pub async fn complete_task(
        &self,
        command: IntegrationTaskCompletionCommand,
        actor: &AuditActor,
    ) -> Result<IntegrationTaskCompletionResult> {
        command.validate()?;
        let receipt = command_identity(
            actor.id(),
            TASK_COMPLETION_AUDIT,
            "work_item",
            &command.work_item_id,
            &command.idempotency_key,
            &command,
        )?;
        super::execution::execute_with_receipt(
            || self.replay_task_completion(&receipt, &command, actor),
            || self.transact_task_completion(command.clone(), actor.clone(), receipt.clone()),
        )
        .await
    }

    async fn transact_task_completion(
        &self,
        command: IntegrationTaskCompletionCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<IntegrationTaskCompletionResult> {
        let attempt_context = super::audit_context(&actor, &receipt)?;
        let rbac = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let prepared = PreparedWorkItemTarget::try_from(&command)?;
        let evidence = std::sync::Arc::clone(&self.evidence);
        let result = self
            .run_audited(move |db, executor| {
                Box::pin(async move {
                    super::execution::run_completion(
                        &mut CompletionCommand {
                            db,
                            evidence: evidence.as_ref(),
                            rbac: &rbac,
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

    async fn replay_task_completion(
        &self,
        receipt: &IntegrationCommandIdentity,
        command: &IntegrationTaskCompletionCommand,
        actor: &AuditActor,
    ) -> Result<Option<IntegrationTaskCompletionResult>> {
        let Some(message) = self.replay_receipt::<CompletionReceiptResult>(receipt, actor).await? else {
            return Ok(None);
        };
        self.ensure_replay_task_access(&command.work_item_id, &command.as_non_terminal_action(), actor)
            .await?;
        Ok(Some(completion_result(command, receipt.receipt_id(), message.terminal_evidence_reference)))
    }
}

fn completion_result(
    command: &IntegrationTaskCompletionCommand,
    receipt_id: &str,
    terminal_reference: String,
) -> IntegrationTaskCompletionResult {
    IntegrationTaskCompletionResult {
        work_item_id: command.work_item_id.clone(),
        work_item_status: IntegrationWorkItemStatus::Completed,
        operation_id: command.decision.operation_id.clone(),
        resolution_record_id: receipt_id.to_string(),
        terminal_evidence_reference: terminal_reference,
    }
}

async fn store_completion_receipt(
    db: &Database,
    actor: &AuditActor,
    receipt: &IntegrationCommandIdentity,
    terminal_reference: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    store_receipt(
        db,
        actor,
        receipt,
        CompletionReceiptResult { terminal_evidence_reference: terminal_reference.to_string() },
        executor,
    )
    .await
}
