//! 非终结动作的回执、正式任务权限与跨域事务。
use super::super::IntegrationResolutionProcess;
use super::guard::{command_identity, load_bound_work_item};
use super::{store_receipt, TASK_ACTION_AUDIT};

use crate::adapters::workflow::work_item_service;
use crate::Result;
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_integration::dto::{
    ControlledEvidenceRef, IntegrationActionOutcome, IntegrationTaskActionCommand,
    IntegrationTaskActionEvidence, IntegrationTaskActionResult, IntegrationWorkItemStatus,
    PreparedWorkItemTarget,
};
use erp_integration::entity::integration_ops::IntegrationCommandIdentity;
use erp_integration::service::task_decision::action::{
    audit_log_reference, execute_task_action, next_allowed_actions, ActionFact,
};
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

/// 将非终态命令各步骤接到正式仓储、权限和注入的证据能力。
struct ActionCommand<'a> {
    db: &'a Database,
    evidence: &'a dyn erp_integration::ports::evidence::IntegrationEvidenceAuthority,
    rbac: &'a erp_identity::SharedRbacService,
    prepared: &'a PreparedWorkItemTarget,
    command: &'a IntegrationTaskActionCommand,
    actor: &'a AuditActor,
    receipt: &'a IntegrationCommandIdentity,
}

#[async_trait::async_trait]
impl super::execution::TaskCommandPort for ActionCommand<'_> {
    type Item = erp_workflow::entity::work_item::WorkItem;
    type Fact = ActionFact;
    type Output = IntegrationTaskActionResult;

    async fn load_bound(&mut self, executor: &mut dyn Executor) -> Result<Self::Item> {
        load_bound_work_item(
            self.db,
            self.prepared,
            &self.command.action,
            self.actor.id(),
            executor,
        )
        .await
    }

    async fn authorize(&mut self, item: &Self::Item, executor: &mut dyn Executor) -> Result<()> {
        work_item_service(self.db.clone(), self.rbac.clone())
            .ensure_domain_decision_access(self.actor, item, executor)
            .await?;
        Ok(())
    }

    async fn apply_domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Fact> {
        Ok(execute_task_action(
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
        if let Some(subject_version) = fact.next_subject_version.clone() {
            item.subject_version = subject_version;
        }
        item.record_activity(self.actor.id(), Instant::now())?;
        Ok(())
    }

    async fn persist_task(&mut self, item: &mut Self::Item, executor: &mut dyn Executor) -> Result<()> {
        self.db.work_items().update(item, executor).await?;
        Ok(())
    }

    fn result(&mut self, fact: &Self::Fact) -> Result<Self::Output> {
        task_action_result(self.command, self.receipt.receipt_id(), fact.clone())
    }

    async fn receipt(&mut self, fact: &Self::Fact, executor: &mut dyn Executor) -> Result<()> {
        store_action_receipt(self.db, self.actor, self.receipt, fact, executor).await
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ActionReceiptMessage {
    #[serde(rename = "o")]
    outcome: IntegrationActionOutcome,
    #[serde(rename = "b", skip_serializing_if = "Option::is_none")]
    business_result_reference: Option<String>,
    #[serde(rename = "e", default, skip_serializing_if = "Vec::is_empty")]
    verified_evidence: Vec<ControlledEvidenceRef>,
}

impl IntegrationResolutionProcess {
    /// 执行 W29 非终结任务动作，并保证任务仍为 `OPEN`。
    ///
    /// # 错误
    /// 责任、任务/主题/领域版本、动作前置条件或幂等指纹不成立时返回错误。
    pub async fn apply_task_action(
        &self,
        command: IntegrationTaskActionCommand,
        actor: &AuditActor,
    ) -> Result<IntegrationTaskActionResult> {
        command.validate()?;
        let receipt = command_identity(
            actor.id(),
            TASK_ACTION_AUDIT,
            "work_item",
            &command.work_item_id,
            &command.idempotency_key,
            &command,
        )?;
        super::execution::execute_with_receipt(
            || self.replay_task_action(&receipt, &command, actor),
            || self.transact_task_action(command.clone(), actor.clone(), receipt.clone()),
        )
        .await
    }

    async fn transact_task_action(
        &self,
        command: IntegrationTaskActionCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<IntegrationTaskActionResult> {
        let rbac = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let prepared = PreparedWorkItemTarget::try_from(&command)?;
        let evidence = std::sync::Arc::clone(&self.evidence);
        self.run_audited(move |db, session| {
            Box::pin(async move {
                super::execution::run_action(
                    &mut ActionCommand {
                        db,
                        evidence: evidence.as_ref(),
                        rbac: &rbac,
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

    async fn replay_task_action(
        &self,
        receipt: &IntegrationCommandIdentity,
        command: &IntegrationTaskActionCommand,
        actor: &AuditActor,
    ) -> Result<Option<IntegrationTaskActionResult>> {
        let Some(message) = self
            .replay_receipt::<ActionReceiptMessage>(receipt, actor)
            .await?
        else {
            return Ok(None);
        };
        let fact = ActionFact {
            outcome: message.outcome,
            business_result_reference: message.business_result_reference,
            next_subject_version: None,
            verified_evidence: message.verified_evidence,
        };
        Ok(Some(task_action_result(command, receipt.receipt_id(), fact)?))
    }
}

fn task_action_result(
    command: &IntegrationTaskActionCommand,
    receipt_id: &str,
    fact: ActionFact,
) -> Result<IntegrationTaskActionResult> {
    Ok(IntegrationTaskActionResult {
        work_item_id: command.work_item_id.clone(),
        work_item_status: IntegrationWorkItemStatus::Open,
        evidence: IntegrationTaskActionEvidence {
            operation_id: command.action.operation_id.clone(),
            outcome: fact.outcome,
            business_result_reference: fact.business_result_reference,
            evidence_reference: Some(audit_log_reference(receipt_id)?),
        },
        next_allowed_actions: next_allowed_actions(command.action.item_type, fact.outcome),
    })
}

async fn store_action_receipt(
    db: &Database,
    actor: &AuditActor,
    receipt: &IntegrationCommandIdentity,
    fact: &ActionFact,
    executor: &mut dyn Executor,
) -> Result<()> {
    let message = ActionReceiptMessage {
        outcome: fact.outcome,
        business_result_reference: fact.business_result_reference.clone(),
        verified_evidence: fact.verified_evidence.clone(),
    };
    store_receipt(db, actor, receipt, message, executor).await
}
