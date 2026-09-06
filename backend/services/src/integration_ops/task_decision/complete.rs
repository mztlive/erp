use database::{IntegrationOpsExt, WorkItemExt};
use entities::integration_ops::{
    DirectConclusion, ErrorTaskStatus, IntegrationCommandIdentity, ResolutionType,
};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::super::evidence::{
    difference_evidence_policy, ensure_completion_policy, error_evidence_policy, verified_reference,
    verify_evidence_refs, EvidenceSubject,
};
use super::super::{
    ControlledEvidenceKind, DirectReconciliationStatus, IntegrationActionOutcome, IntegrationItemType,
    IntegrationOpsService, IntegrationTaskCompletionCommand, IntegrationTaskCompletionResult,
    IntegrationWorkItemStatus, PreparedWorkItemTarget,
};
use super::guard::{
    command_identity, ensure_difference_open, ensure_difference_subject, ensure_error_task_subject,
    latest_resolution, load_bound_work_item, load_difference, load_error_task,
};
use super::{append_resolution, store_receipt, DirectFact, TASK_COMPLETION_AUDIT};
use crate::errors::Result;
use crate::work_item::WorkItemService;
use application_core::AuditActor;

#[derive(Debug)]
struct TerminalFact {
    reference: String,
    next_subject_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompletionReceiptMessage {
    #[serde(rename = "e")]
    terminal_evidence_reference: String,
}

impl IntegrationOpsService {
    /// 执行 W29 任务完成强命令。
    ///
    /// # 错误
    /// 无法由当前权威事实验证终态，或责任/版本/幂等校验失败时返回错误。
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
        if let Some(result) = self.replay_task_completion(&receipt, &command, actor).await? {
            return Ok(result);
        }
        let result = self
            .transact_task_completion(command.clone(), actor.clone(), receipt.clone())
            .await;
        self.recover_task_completion(result, &receipt, &command, actor)
            .await
    }

    async fn transact_task_completion(
        &self,
        command: IntegrationTaskCompletionCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<IntegrationTaskCompletionResult> {
        let rbac = crate::identity_compose::shared_rbac_service(self.db.clone());
        let prepared = PreparedWorkItemTarget::try_from(&command)?;
        self.run_audited(move |db, session| {
            Box::pin(async move {
                let action = command.as_non_terminal_action();
                let mut work_item = load_bound_work_item(db, &prepared, &action, actor.id(), session).await?;
                WorkItemService::new(db.clone(), rbac.clone())
                    .ensure_domain_decision_access(&actor, &work_item, session)
                    .await?;
                let terminal =
                    complete_domain_item(db, &command, receipt.receipt_id(), actor.id(), session).await?;
                work_item.subject_version = terminal.next_subject_version;
                work_item.complete_by_domain_command(actor.id(), Instant::now())?;
                db.work_items().update(&mut work_item, session).await?;
                store_completion_receipt(db, &actor, &receipt, &terminal.reference, session).await?;
                Ok(completion_result(
                    &command,
                    receipt.receipt_id(),
                    terminal.reference,
                ))
            })
        })
        .await
    }

    async fn replay_task_completion(
        &self,
        receipt: &IntegrationCommandIdentity,
        command: &IntegrationTaskCompletionCommand,
        actor: &AuditActor,
    ) -> Result<Option<IntegrationTaskCompletionResult>> {
        let Some(message) = self
            .replay_receipt::<CompletionReceiptMessage>(receipt, actor)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(completion_result(
            command,
            receipt.receipt_id(),
            message.terminal_evidence_reference,
        )))
    }

    async fn recover_task_completion(
        &self,
        result: Result<IntegrationTaskCompletionResult>,
        receipt: &IntegrationCommandIdentity,
        command: &IntegrationTaskCompletionCommand,
        actor: &AuditActor,
    ) -> Result<IntegrationTaskCompletionResult> {
        match result {
            Ok(result) => Ok(result),
            Err(error) => match self.replay_task_completion(receipt, command, actor).await? {
                Some(result) => Ok(result),
                None => Err(error),
            },
        }
    }
}

async fn complete_domain_item(
    db: &Database,
    command: &IntegrationTaskCompletionCommand,
    resolution_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<TerminalFact> {
    match command.decision.item_type {
        IntegrationItemType::ErrorTask => complete_error_task(db, command, actor_id, executor).await,
        IntegrationItemType::ReconciliationDifference => {
            complete_difference(db, command, resolution_id, actor_id, executor).await
        }
    }
}

async fn complete_error_task(
    db: &Database,
    command: &IntegrationTaskCompletionCommand,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<TerminalFact> {
    let mut task = load_error_task(db, &command.decision.item_id, executor).await?;
    ensure_error_task_subject(&task, &command.expected_subject_version)?;
    let policy = error_evidence_policy(&task);
    ensure_completion_policy(
        &command.decision.evidence_policy_id,
        command.decision.evidence_policy_version,
        &command.decision.policy_key,
        &command.decision.evidence_refs,
        &policy,
    )?;
    let subject = EvidenceSubject::error(&task);
    let verified =
        verify_evidence_refs(db, &subject, &command.decision.evidence_refs, actor_id, executor).await?;
    let reference = verified_reference(&verified)?;
    let resolution = completion_resolution(command, &reference, actor_id);
    let resolution_type = ResolutionType::from_verified_evidence(
        verified
            .iter()
            .any(|evidence| evidence.reference.kind == ControlledEvidenceKind::CompensationResult),
        verified
            .iter()
            .any(|evidence| evidence.reference.kind == ControlledEvidenceKind::BusinessObjectVerification),
    );
    task.transition(
        ErrorTaskStatus::Resolved,
        Some(resolution_type),
        Some(resolution),
        Instant::now(),
    )?;
    db.integration_error_tasks().update(&mut task, executor).await?;
    Ok(TerminalFact {
        reference,
        next_subject_version: task.base.version.to_string(),
    })
}

async fn complete_difference(
    db: &Database,
    command: &IntegrationTaskCompletionCommand,
    resolution_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<TerminalFact> {
    let difference = load_difference(db, &command.decision.item_id, executor).await?;
    let latest = latest_resolution(db, &command.decision.item_id, executor).await?;
    ensure_difference_subject(latest.as_ref(), &command.expected_subject_version)?;
    ensure_difference_open(latest.as_ref())?;
    let policy = difference_evidence_policy(&difference);
    ensure_completion_policy(
        &command.decision.evidence_policy_id,
        command.decision.evidence_policy_version,
        &command.decision.policy_key,
        &command.decision.evidence_refs,
        &policy,
    )?;
    let subject = EvidenceSubject::difference(&difference);
    let verified =
        verify_evidence_refs(db, &subject, &command.decision.evidence_refs, actor_id, executor).await?;
    let reference = verified_reference(&verified)?;
    let typed = DirectConclusion::ConfirmValidDifference;
    let fact = DirectFact {
        action: typed.resolution_action(),
        evidence_reference: Some(reference.clone()),
        resulting_status: DirectReconciliationStatus::ConfirmedValidDifference,
        outcome: IntegrationActionOutcome::ConfirmedValidDifference,
        business_result_reference: Some(reference.clone()),
        verified_evidence: verified.into_iter().map(|evidence| evidence.reference).collect(),
    };
    let record = append_resolution(&difference, latest.as_ref(), &fact, resolution_id, actor_id)?;
    let next_subject_version = record.resolution_no.to_string();
    db.reconciliation_difference_resolutions()
        .create(&record, executor)
        .await?;
    Ok(TerminalFact {
        reference,
        next_subject_version,
    })
}

fn completion_resolution(
    command: &IntegrationTaskCompletionCommand,
    terminal_reference: &str,
    actor_id: &str,
) -> String {
    format!(
        "operation={};reason_code={};terminal_evidence={};actor={}",
        command.decision.operation_id,
        command.decision.reason_code.as_str(),
        terminal_reference,
        actor_id
    )
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
        CompletionReceiptMessage {
            terminal_evidence_reference: terminal_reference.to_string(),
        },
        executor,
    )
    .await
}
