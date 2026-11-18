//! 错误任务动作与差异非终态决定的本域实施。
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use super::guard::{
    ensure_difference_open, ensure_difference_subject, ensure_error_task_subject, latest_resolution,
    load_difference, load_error_task,
};
use super::{DirectFact, persist_appended_resolution};
use crate::dto::{
    ControlledEvidenceKind, ControlledEvidenceRef, DirectReconciliationStatus, IntegrationActionOutcome,
    IntegrationItemType, IntegrationNonTerminalTaskAction, IntegrationTaskActionCommand,
    IntegrationTaskActionKind,
};
use crate::entity::integration_ops::{
    CompactEvidenceSet, EvidenceRecordRef, IntegrationErrorTask, ProjectionOutcome, ProjectionSubject,
    ReconciliationDifference, ResolutionAction, next_actions_after_outcome,
};
use crate::ports::evidence::{
    EvidenceSubject, IntegrationEvidenceAuthority, OriginalResultFact, VerifiedEvidence,
};
use crate::repository::IntegrationOpsExt;
use crate::service::evidence::{verified_reference, verify_evidence_refs};
use crate::{Error, Result};

/// 本域动作事实；WorkItem 活动与回执仍由流程写入。
#[derive(Debug, Clone)]
pub struct ActionFact {
    /// 本次动作的稳定结果代码。
    pub outcome: IntegrationActionOutcome,
    /// 权威业务结果引用。
    pub business_result_reference: Option<String>,
    /// 本域写入后的主题版本；回执回放时不更新任务。
    pub next_subject_version: Option<String>,
    /// 已按原顺序验证的证据。
    pub verified_evidence: Vec<ControlledEvidenceRef>,
}

impl ActionFact {
    // 仅结果代码的事实（无业务引用、无证据）。返回无引用无证据的动作事实。
    //
    // `outcome` 为本次动作的稳定结果代码。
    fn outcome_only(outcome: IntegrationActionOutcome) -> Self {
        Self {
            outcome,
            business_result_reference: None,
            next_subject_version: None,
            verified_evidence: Vec::new(),
        }
    }

    // 携带业务结果引用的事实（无证据）。返回带业务引用的动作事实。
    //
    // `outcome` 为本次动作的稳定结果代码。
    fn with_reference(outcome: IntegrationActionOutcome, reference: String) -> Self {
        Self { business_result_reference: Some(reference), ..Self::outcome_only(outcome) }
    }

    // 携带已验证证据的事实（业务引用取证据集合引用）。
    //
    // 返回带证据引用列表的动作事实。证据集合引用语法非法时返回校验错误。
    //
    // - `outcome`：本次动作的稳定结果代码。
    // - `verified`：已验证证据（保持请求顺序）。
    fn with_verified(outcome: IntegrationActionOutcome, verified: Vec<VerifiedEvidence>) -> Result<Self> {
        let business_result_reference = Some(verified_reference(&verified)?);
        Ok(Self {
            business_result_reference,
            verified_evidence: verified_refs(verified),
            ..Self::outcome_only(outcome)
        })
    }
}

// 把已验证证据投影为引用列表（保持请求顺序）。
fn verified_refs(verified: Vec<VerifiedEvidence>) -> Vec<ControlledEvidenceRef> {
    verified.into_iter().map(|evidence| evidence.reference).collect()
}

/// 正式任务关联由调用方确认后，按错误任务或对账差异执行本域非终结动作。
/// 用原执行器核验主题版本、开放状态和证据并完成写入；差异以命令回执 ID 作为追加决定的主键。
/// 返回动作事实及写入后的主题版本，不在本入口写正式任务活动或命令回执。
pub async fn execute_task_action(
    db: &Database,
    authority: &dyn IntegrationEvidenceAuthority,
    command: &IntegrationTaskActionCommand,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    match command.action.item_type {
        IntegrationItemType::ErrorTask => {
            execute_error_task_action(db, authority, command, actor_id, executor).await
        },
        IntegrationItemType::ReconciliationDifference => {
            execute_difference_task_action(db, authority, command, receipt_id, actor_id, executor).await
        },
    }
}

async fn execute_error_task_action(
    db: &Database,
    authority: &dyn IntegrationEvidenceAuthority,
    command: &IntegrationTaskActionCommand,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    let mut task = load_error_task(db, &command.action.item_id, executor).await?;
    ensure_error_task_subject(&task, &command.expected_subject_version)?;

    let mut fact = error_action_fact(authority, &task, &command.action, actor_id, executor).await?;
    let summary = error_action_summary(&command.action, &fact)?;
    task.record_attempt(Instant::now(), Some(summary))?;
    db.integration_error_tasks().update(&mut task, executor).await?;
    fact.next_subject_version = Some(task.base.version.to_string());
    Ok(fact)
}

async fn error_action_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    task: &IntegrationErrorTask,
    action: &IntegrationNonTerminalTaskAction,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    let subject = EvidenceSubject::error(task);
    match action.kind {
        IntegrationTaskActionKind::QueryOriginalResult => {
            query_action_fact(authority, &subject, executor).await
        },
        IntegrationTaskActionKind::AddEvidence => {
            let verified =
                verify_evidence_refs(authority, &subject, &action.evidence_refs, actor_id, executor).await?;
            ActionFact::with_verified(IntegrationActionOutcome::EvidenceAdded, verified)
        },
        IntegrationTaskActionKind::ReplayOriginal => {
            if !task.can_replay_original() {
                return Err(Error::BusinessLogicError(
                    "只有确认原操作没有产生结果，且此类失败允许重新处理时才能继续，请先查询原操作结果"
                        .to_string(),
                ));
            }
            let reference = authority.replay_original(&subject, executor).await?;
            Ok(ActionFact::with_reference(IntegrationActionOutcome::ReplayAccepted, reference))
        },
        IntegrationTaskActionKind::Reattribute => {
            let reference = authority.verify_reattribution(&subject, executor).await?;
            Ok(ActionFact {
                outcome: IntegrationActionOutcome::Reattributed,
                business_result_reference: Some(reference),
                next_subject_version: None,
                verified_evidence: authority.discover_evidence(&subject, executor).await?,
            })
        },
        IntegrationTaskActionKind::LinkCompensation => {
            ensure_compensation_evidence(&action.evidence_refs)?;
            let verified =
                verify_evidence_refs(authority, &subject, &action.evidence_refs, actor_id, executor).await?;
            ActionFact::with_verified(IntegrationActionOutcome::EvidenceLinked, verified)
        },
    }
}

async fn query_action_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    subject: &EvidenceSubject,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    match authority.query_original(subject, executor).await? {
        OriginalResultFact::Terminal(reference) => Ok(ActionFact {
            outcome: IntegrationActionOutcome::TerminalEvidenceFound,
            business_result_reference: Some(reference),
            next_subject_version: None,
            verified_evidence: authority.discover_evidence(subject, executor).await?,
        }),
        OriginalResultFact::NoResult => {
            Ok(ActionFact::outcome_only(IntegrationActionOutcome::NoResultConfirmed))
        },
        OriginalResultFact::Unknown => Ok(ActionFact::outcome_only(IntegrationActionOutcome::ResultUnknown)),
    }
}

/// 动作证据摘要上限（字节口径；摘要为 ASCII 固定前缀 + 紧凑证据编码，字节 == 字符）。
const ACTION_SUMMARY_MAX_BYTES: usize = 512;

// 组装动作摘要；紧凑证据非法或结果超过 512 字节时失败。
fn error_action_summary(action: &IntegrationNonTerminalTaskAction, fact: &ActionFact) -> Result<String> {
    let evidence = compact_evidence(&action.evidence_refs)?;
    let summary = format!(
        "w29_action={};operation={};outcome={};evidence={}",
        action.kind.as_str(),
        action.operation_id,
        fact.outcome.as_str(),
        evidence.unwrap_or_else(|| "none".to_string())
    );
    if summary.len() > ACTION_SUMMARY_MAX_BYTES {
        return Err(Error::ValidationError("动作证据摘要过长".to_string()));
    }
    Ok(summary)
}

async fn execute_difference_task_action(
    db: &Database,
    authority: &dyn IntegrationEvidenceAuthority,
    command: &IntegrationTaskActionCommand,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    let difference = load_difference(db, &command.action.item_id, executor).await?;
    let latest = latest_resolution(db, &command.action.item_id, executor).await?;
    ensure_difference_subject(latest.as_ref(), &command.expected_subject_version)?;
    ensure_difference_open(latest.as_ref())?;

    let fact =
        difference_action_fact(authority, &difference, &command.action, receipt_id, actor_id, executor)
            .await?;

    let next_subject_version =
        persist_appended_resolution(db, &difference, latest.as_ref(), &fact, receipt_id, actor_id, executor)
            .await?;
    Ok(ActionFact {
        outcome: fact.outcome,
        business_result_reference: fact.business_result_reference,
        next_subject_version: Some(next_subject_version),
        verified_evidence: fact.verified_evidence,
    })
}

// 开放态差异决定事实（结果状态 `Open`）。返回结果状态为开放的差异决定事实。
//
// - `fact`：同动作的本域动作事实（结果与引用透传）。
// - `evidence_reference`：决定记录中的已验证证据引用。
fn open_direct_fact(
    action: ResolutionAction,
    fact: ActionFact,
    evidence_reference: Option<String>,
) -> DirectFact {
    DirectFact {
        action,
        evidence_reference,
        resulting_status: DirectReconciliationStatus::Open,
        outcome: fact.outcome,
        business_result_reference: fact.business_result_reference,
        verified_evidence: fact.verified_evidence,
    }
}

// 待证差异决定事实（结果状态 `EvidencePending`）。返回结果状态为待证的差异决定事实。
//
// - `evidence_reference`：决定记录中的已验证证据引用。
// - `outcome`：本次动作的稳定结果代码。
// - `business_result_reference`：权威业务结果的稳定引用。
fn pending_direct_fact(
    action: ResolutionAction,
    evidence_reference: Option<String>,
    outcome: IntegrationActionOutcome,
    business_result_reference: Option<String>,
    verified_evidence: Vec<ControlledEvidenceRef>,
) -> DirectFact {
    DirectFact {
        action,
        evidence_reference,
        resulting_status: DirectReconciliationStatus::EvidencePending,
        outcome,
        business_result_reference,
        verified_evidence,
    }
}

// 按非终结动作种类构造差异决定事实，不在此写入决定记录。
//
// 返回尚未持久化的 `DirectFact`。权威查询、重放、归集或证据验证失败，补偿证据缺失，或回执引用 grammar 非法时返回对应错误。
//
// 沿用调用方的执行器，不另开事务。
pub(super) async fn difference_action_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    difference: &ReconciliationDifference,
    action: &IntegrationNonTerminalTaskAction,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    let subject = EvidenceSubject::difference(difference);
    match action.kind {
        IntegrationTaskActionKind::QueryOriginalResult => {
            difference_query_fact(authority, &subject, receipt_id, executor).await
        },
        IntegrationTaskActionKind::AddEvidence => {
            difference_add_evidence_fact(authority, &subject, action, actor_id, executor).await
        },
        IntegrationTaskActionKind::ReplayOriginal => {
            difference_replay_fact(authority, &subject, executor).await
        },
        IntegrationTaskActionKind::Reattribute => {
            difference_reattribute_fact(authority, &subject, executor).await
        },
        IntegrationTaskActionKind::LinkCompensation => {
            difference_link_compensation_fact(authority, &subject, action, actor_id, executor).await
        },
    }
}

// 执行差异的“查询原结果”动作：读取权威结果，用本次命令回执作为决定证据，差异仍保持开放。
async fn difference_query_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    subject: &EvidenceSubject,
    receipt_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    let fact = query_action_fact(authority, subject, executor).await?;
    let evidence_reference = Some(command_receipt_reference(receipt_id)?);
    Ok(open_direct_fact(ResolutionAction::QueryOriginalResult, fact, evidence_reference))
}

// 在差异补证时逐条重验提交的引用并生成可持久化证据；本次决定保持待证，不终结差异。
async fn difference_add_evidence_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    subject: &EvidenceSubject,
    action: &IntegrationNonTerminalTaskAction,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    let verified =
        verify_evidence_refs(authority, subject, &action.evidence_refs, actor_id, executor).await?;
    let evidence = verified_reference(&verified)?;
    Ok(pending_direct_fact(
        ResolutionAction::AddEvidence,
        Some(evidence),
        IntegrationActionOutcome::EvidenceAdded,
        None,
        verified_refs(verified),
    ))
}

// 沿原差异证据上下文请求权威端口重放；返回受理引用，不把重放受理视为问题已解决。
async fn difference_replay_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    subject: &EvidenceSubject,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    let reference = authority.replay_original(subject, executor).await?;
    Ok(open_direct_fact(
        ResolutionAction::ReplayOriginal,
        ActionFact::with_reference(IntegrationActionOutcome::ReplayAccepted, reference),
        None,
    ))
}

// 由权威端口核验已归集结果并补齐可见证据，供差异追加待证决定。
async fn difference_reattribute_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    subject: &EvidenceSubject,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    let reference = authority.verify_reattribution(subject, executor).await?;
    Ok(pending_direct_fact(
        ResolutionAction::Reattribute,
        Some(reference.clone()),
        IntegrationActionOutcome::Reattributed,
        Some(reference),
        authority.discover_evidence(subject, executor).await?,
    ))
}

// 关联差异补偿时，先要求补偿结果证据，再逐条交权威端口重验；可记录关联，但不在此终结差异。
async fn difference_link_compensation_fact(
    authority: &dyn IntegrationEvidenceAuthority,
    subject: &EvidenceSubject,
    action: &IntegrationNonTerminalTaskAction,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    ensure_compensation_evidence(&action.evidence_refs)?;
    let verified =
        verify_evidence_refs(authority, subject, &action.evidence_refs, actor_id, executor).await?;
    let reference = verified_reference(&verified)?;
    Ok(pending_direct_fact(
        ResolutionAction::LinkCompensation,
        Some(reference.clone()),
        IntegrationActionOutcome::EvidenceLinked,
        Some(reference),
        verified_refs(verified),
    ))
}

// 关联补偿必须携带补偿结果证据（两处决定入口共用，见 [`super::direct`]）。
//
// 至少一条为 `CompensationResult` 时成功。没有任何补偿结果证据时返回 `ValidationError`。
pub(super) fn ensure_compensation_evidence(refs: &[ControlledEvidenceRef]) -> Result<()> {
    if refs.iter().any(|evidence| evidence.kind == ControlledEvidenceKind::CompensationResult) {
        Ok(())
    } else {
        Err(Error::ValidationError("关联补偿必须提供补偿结果证据".to_string()))
    }
}

/// 由收据 ID 构造 `integration_command_receipt:id` 证据记录引用。
///
/// 语法由 [`EvidenceRecordRef`] 独占，禁止 `format!` 拼接。
///
/// 返回精确 `type:id` 编码。收据 ID 不符合证据记录 grammar 时返回校验错误。
pub fn command_receipt_reference(receipt_id: &str) -> Result<String> {
    EvidenceRecordRef::new("integration_command_receipt", receipt_id)
        .map(|reference| reference.to_string())
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 推导单次动作后的下一开放动作代码（推导规则归领域，此处只做代码映射）。
pub fn next_allowed_actions(
    item_type: IntegrationItemType,
    outcome: IntegrationActionOutcome,
) -> Vec<String> {
    let subject = match item_type {
        IntegrationItemType::ErrorTask => ProjectionSubject::ErrorTask,
        IntegrationItemType::ReconciliationDifference => ProjectionSubject::ReconciliationDifference,
    };
    let projected = match outcome {
        IntegrationActionOutcome::TerminalEvidenceFound => ProjectionOutcome::TerminalEvidenceFound,
        IntegrationActionOutcome::NoResultConfirmed => ProjectionOutcome::NoResultConfirmed,
        _ => ProjectionOutcome::Other,
    };
    next_actions_after_outcome(subject, projected).iter().map(|action| action.as_str().to_string()).collect()
}

// 将动作证据引用规范化为可写入摘要的紧凑集合。
//
// grammar、排序、去重与长度由 [`CompactEvidenceSet`] 独占。
//
// 空集合返回 `Ok(None)`；否则返回排序去重后的紧凑编码。记录 ID 非法或编码超过 512 字节时返回校验错误。
fn compact_evidence(refs: &[ControlledEvidenceRef]) -> Result<Option<String>> {
    CompactEvidenceSet::try_from_pairs(
        refs.iter().map(|evidence| (evidence.kind.as_str(), evidence.record_id.as_str())),
    )
    .map(|set| set.map(CompactEvidenceSet::into_wire))
    .map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod tests;
