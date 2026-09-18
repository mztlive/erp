//! W29 本域终态证据验证、错误任务解决与差异决定追加。
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use super::guard::{
    ensure_difference_open, ensure_difference_subject, ensure_error_task_subject, latest_resolution,
    load_difference, load_error_task,
};
use super::{DirectFact, persist_appended_resolution};
use crate::Result;
use crate::dto::{
    ControlledEvidenceKind, DirectReconciliationStatus, IntegrationActionOutcome, IntegrationItemType,
    IntegrationTaskCompletionCommand,
};
use crate::entity::integration_ops::{DirectConclusion, ErrorTaskStatus, ResolutionType};
use crate::ports::evidence::{EvidenceSubject, IntegrationEvidenceAuthority};
use crate::repository::IntegrationOpsExt;
use crate::service::evidence::{
    difference_evidence_policy, ensure_completion_policy, error_evidence_policy, verified_reference,
    verify_evidence_refs,
};

/// 本域已形成的终态；供正式任务完成和回执使用。
#[derive(Debug)]
pub struct TerminalFact {
    /// 已验证终态证据。
    pub reference: String,
    /// 本域写入后的主题版本。
    pub next_subject_version: String,
}

/// 验证证据并终结本域对象，沿原执行器写入后返回主题版本。
pub async fn complete_domain_item(
    db: &Database,
    authority: &dyn IntegrationEvidenceAuthority,
    command: &IntegrationTaskCompletionCommand,
    resolution_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<TerminalFact> {
    match command.decision.item_type {
        IntegrationItemType::ErrorTask => {
            complete_error_task(db, authority, command, actor_id, executor).await
        },
        IntegrationItemType::ReconciliationDifference => {
            complete_difference(db, authority, command, resolution_id, actor_id, executor).await
        },
    }
}

async fn complete_error_task(
    db: &Database,
    authority: &dyn IntegrationEvidenceAuthority,
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
        verify_evidence_refs(authority, &subject, &command.decision.evidence_refs, actor_id, executor)
            .await?;
    let reference = verified_reference(&verified)?;
    let resolution = completion_resolution(command, &reference, actor_id);
    let resolution_type = ResolutionType::from_verified_evidence(
        verified.iter().any(|evidence| evidence.reference.kind == ControlledEvidenceKind::CompensationResult),
        verified
            .iter()
            .any(|evidence| evidence.reference.kind == ControlledEvidenceKind::BusinessObjectVerification),
    );
    task.transition(ErrorTaskStatus::Resolved, Some(resolution_type), Some(resolution), Instant::now())?;
    task.record_completed_by(actor_id.to_string())?;
    db.integration_error_tasks().update(&mut task, executor).await?;
    Ok(TerminalFact { reference, next_subject_version: task.base.version.to_string() })
}

async fn complete_difference(
    db: &Database,
    authority: &dyn IntegrationEvidenceAuthority,
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
        verify_evidence_refs(authority, &subject, &command.decision.evidence_refs, actor_id, executor)
            .await?;
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
    let next_subject_version = persist_appended_resolution(
        db,
        &difference,
        latest.as_ref(),
        &fact,
        resolution_id,
        actor_id,
        executor,
    )
    .await?;
    Ok(TerminalFact { reference, next_subject_version })
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
