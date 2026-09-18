//! 无任务直接对账的本域版本、证据和追加决定。
use mongodb::Database;
use persistence_core::Executor;

use super::action::difference_action_fact;
use super::guard::{ensure_difference_open, latest_resolution, load_difference};
use super::{DirectFact, persist_appended_resolution};
use crate::dto::{
    ControlledEvidenceRef, DifferenceReasonCode, DirectReconciliationCommand, DirectReconciliationConclusion,
    DirectReconciliationDecision, DirectReconciliationStatus, IntegrationActionOutcome, IntegrationItemType,
    IntegrationNonTerminalTaskAction, PreparedDirectDecisionTarget,
};
use crate::entity::integration_ops::{
    DirectConclusion, ReconciliationDifference, ReconciliationDifferenceResolution,
};
use crate::ports::evidence::{EvidenceSubject, IntegrationEvidenceAuthority};
use crate::service::evidence::{ensure_direct_reason, verified_reference, verify_evidence_refs};
use crate::{Error, Result};

/// 正式任务关联已由流程检查后，按原顺序读取版本、验证证据并追加决定。
pub async fn execute_direct_decision(
    db: &Database,
    authority: &dyn IntegrationEvidenceAuthority,
    prepared: &PreparedDirectDecisionTarget,
    command: &DirectReconciliationCommand,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    let difference = load_difference(db, &prepared.difference_id, executor).await?;
    let latest = latest_resolution(db, &prepared.difference_id, executor).await?;
    ensure_direct_version(prepared.difference_version, latest.as_ref())?;
    ensure_difference_open(latest.as_ref())?;
    let fact = direct_decision_fact(authority, &difference, command, receipt_id, actor_id, executor).await?;
    persist_appended_resolution(db, &difference, latest.as_ref(), &fact, receipt_id, actor_id, executor)
        .await?;
    Ok(fact)
}

/// 校验直接对账差异版本与当前决定序号一致（typed 比较，无二次解析）。
///
/// 非十进制输入已由 Prepared 目标在 DTO 层拒绝；此处只区分当前与陈旧。
fn ensure_direct_version(expected: u64, latest: Option<&ReconciliationDifferenceResolution>) -> Result<()> {
    if ReconciliationDifferenceResolution::current_version(latest) == expected {
        Ok(())
    } else {
        Err(Error::ConflictError("差异决定版本已变化，请刷新后重试".to_string()))
    }
}

async fn direct_decision_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    difference: &ReconciliationDifference,
    command: &DirectReconciliationCommand,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    match &command.decision {
        DirectReconciliationDecision::NonTerminalAction { action, evidence_refs, comment: _ } => {
            difference_action_fact(
                authority,
                difference,
                &IntegrationNonTerminalTaskAction {
                    item_type: IntegrationItemType::ReconciliationDifference,
                    item_id: command.difference_id.clone(),
                    kind: *action,
                    operation_id: command.operation_id.clone(),
                    reason_code: None,
                    comment: None,
                    evidence_refs: evidence_refs.clone(),
                },
                receipt_id,
                actor_id,
                executor,
            )
            .await
        },
        DirectReconciliationDecision::TerminalConclusion {
            conclusion,
            reason_code,
            reason_registry_id,
            reason_registry_version,
            registered_reason_id,
            evidence_refs,
            comment: _,
        } => {
            terminal_conclusion_fact(
                authority,
                difference,
                TerminalConclusionInput {
                    conclusion: *conclusion,
                    reason_code: *reason_code,
                    reason_registry_id,
                    reason_registry_version: *reason_registry_version,
                    registered_reason_id,
                    evidence_refs,
                },
                actor_id,
                executor,
            )
            .await
        },
    }
}

/// 终结结论分支输入（`terminal_conclusion_fact` 7 参瘦身）。
struct TerminalConclusionInput<'a> {
    /// 固定结论。
    conclusion: DirectReconciliationConclusion,
    /// 固定原因代码。
    reason_code: DifferenceReasonCode,
    /// 原因注册表身份。
    reason_registry_id: &'a str,
    /// 原因注册表版本。
    reason_registry_version: u64,
    /// 注册原因 ID。
    registered_reason_id: &'a str,
    /// 待验证证据引用。
    evidence_refs: &'a [ControlledEvidenceRef],
}

/// 终结结论分支（门禁 50 行内拆分）。
async fn terminal_conclusion_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    difference: &ReconciliationDifference,
    input: TerminalConclusionInput<'_>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    ensure_direct_reason(
        input.reason_registry_id,
        input.reason_registry_version,
        input.registered_reason_id,
        input.reason_code,
        input.conclusion,
        input.evidence_refs,
    )?;
    let subject = EvidenceSubject::difference(difference);
    let verified = verify_evidence_refs(authority, &subject, input.evidence_refs, actor_id, executor).await?;
    let reference = verified_reference(&verified)?;
    let typed = DirectConclusion::from(input.conclusion);
    let (resulting_status, outcome) = match input.conclusion {
        DirectReconciliationConclusion::ConfirmNoError => {
            (DirectReconciliationStatus::ConfirmedNoError, IntegrationActionOutcome::ConfirmedNoError)
        },
        DirectReconciliationConclusion::ConfirmValidDifference => (
            DirectReconciliationStatus::ConfirmedValidDifference,
            IntegrationActionOutcome::ConfirmedValidDifference,
        ),
    };
    Ok(DirectFact {
        action: typed.resolution_action(),
        evidence_reference: Some(reference.clone()),
        resulting_status,
        outcome,
        business_result_reference: Some(reference),
        verified_evidence: verified.into_iter().map(|evidence| evidence.reference).collect(),
    })
}
