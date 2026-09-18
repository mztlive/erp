//! 无正式任务对账的任务关联闸门、事务与回执。
use application_core::AuditActor;
use erp_integration::dto::{
    DirectReconciliationCommand, DirectReconciliationResult, DirectReconciliationStatus,
    IntegrationActionOutcome, PreparedDirectDecisionTarget,
};
use erp_integration::entity::integration_ops::IntegrationCommandIdentity;
use erp_integration::service::task_decision::DirectFact;
use erp_integration::service::task_decision::direct::execute_direct_decision;
use erp_workflow::WorkItemExt;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize)]
struct DirectReceiptMessage {
    #[serde(rename = "s")]
    resulting_status: DirectReconciliationStatus,
    #[serde(rename = "t")]
    is_terminal: bool,
    #[serde(rename = "o")]
    outcome: IntegrationActionOutcome,
    #[serde(rename = "b", skip_serializing_if = "Option::is_none")]
    business_result_reference: Option<String>,
}

impl IntegrationResolutionProcess {
    /// 对未关联任何正式任务的差异提交 decision-only 命令。
    ///
    /// # 错误
    /// 路径身份、差异版本、任务关联、终态证据或幂等校验失败时返回错误。
    pub async fn decide_difference(
        &self,
        path_id: &str,
        command: DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<DirectReconciliationResult> {
        command.validate()?;
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

    async fn transact_direct_decision(
        &self,
        command: DirectReconciliationCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<DirectReconciliationResult> {
        let evidence = std::sync::Arc::clone(&self.evidence);
        self.run_audited(move |db, session| {
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
                    session,
                )
                .await
            })
        })
        .await
    }

    async fn replay_direct_decision(
        &self,
        receipt: &IntegrationCommandIdentity,
        command: &DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<Option<DirectReconciliationResult>> {
        let Some(message) = self.replay_receipt::<DirectReceiptMessage>(receipt, actor).await? else {
            return Ok(None);
        };
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
        DirectReceiptMessage {
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
