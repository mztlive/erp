use database::{IntegrationOpsExt, WorkItemExt};
use entities::integration_ops::{
    next_actions_after_outcome, CompactEvidenceSet, EvidenceRecordRef, IntegrationCommandIdentity,
    IntegrationErrorTask, ProjectionOutcome, ProjectionSubject, ReconciliationDifference, ResolutionAction,
};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::super::evidence::{
    verified_reference, verify_evidence_refs, EvidenceSubject, IntegrationEvidenceAuthority,
    OriginalResultFact,
};
use super::super::{
    ControlledEvidenceKind, ControlledEvidenceRef, DirectReconciliationStatus, IntegrationActionOutcome,
    IntegrationItemType, IntegrationNonTerminalTaskAction, IntegrationOpsService,
    IntegrationTaskActionCommand, IntegrationTaskActionEvidence, IntegrationTaskActionKind,
    IntegrationTaskActionResult, IntegrationWorkItemStatus, PreparedWorkItemTarget,
};
use super::guard::{
    command_identity, ensure_difference_open, ensure_difference_subject, ensure_error_task_subject,
    latest_resolution, load_bound_work_item, load_difference, load_error_task,
};
use super::{append_resolution, store_receipt, DirectFact, TASK_ACTION_AUDIT};
use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;
use application_core::AuditActor;

#[derive(Debug, Clone)]
struct ActionFact {
    outcome: IntegrationActionOutcome,
    business_result_reference: Option<String>,
    next_subject_version: Option<String>,
    verified_evidence: Vec<ControlledEvidenceRef>,
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

impl IntegrationOpsService {
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
        if let Some(result) = self.replay_task_action(&receipt, &command, actor).await? {
            return Ok(result);
        }
        let result = self
            .transact_task_action(command.clone(), actor.clone(), receipt.clone())
            .await;
        self.recover_task_action(result, &receipt, &command, actor).await
    }

    async fn transact_task_action(
        &self,
        command: IntegrationTaskActionCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<IntegrationTaskActionResult> {
        let rbac = crate::identity_compose::shared_rbac_service(self.db.clone());
        let prepared = PreparedWorkItemTarget::try_from(&command)?;
        self.run_audited(move |db, session| {
            Box::pin(async move {
                let mut work_item =
                    load_bound_work_item(db, &prepared, &command.action, actor.id(), session).await?;
                WorkItemService::new(db.clone(), rbac.clone())
                    .ensure_domain_decision_access(&actor, &work_item, session)
                    .await?;
                let fact =
                    execute_task_action(db, &command, receipt.receipt_id(), actor.id(), session).await?;
                if let Some(subject_version) = fact.next_subject_version.clone() {
                    work_item.subject_version = subject_version;
                }
                work_item.record_activity(actor.id(), Instant::now())?;
                db.work_items().update(&mut work_item, session).await?;
                let result = task_action_result(&command, receipt.receipt_id(), fact.clone())?;
                store_action_receipt(db, &actor, &receipt, &fact, session).await?;
                Ok(result)
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

    async fn recover_task_action(
        &self,
        result: Result<IntegrationTaskActionResult>,
        receipt: &IntegrationCommandIdentity,
        command: &IntegrationTaskActionCommand,
        actor: &AuditActor,
    ) -> Result<IntegrationTaskActionResult> {
        match result {
            Ok(result) => Ok(result),
            Err(error) => match self.replay_task_action(receipt, command, actor).await? {
                Some(result) => Ok(result),
                None => Err(error),
            },
        }
    }
}

async fn execute_task_action(
    db: &Database,
    command: &IntegrationTaskActionCommand,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    match command.action.item_type {
        IntegrationItemType::ErrorTask => execute_error_task_action(db, command, actor_id, executor).await,
        IntegrationItemType::ReconciliationDifference => {
            execute_difference_task_action(db, command, receipt_id, actor_id, executor).await
        }
    }
}

async fn execute_error_task_action(
    db: &Database,
    command: &IntegrationTaskActionCommand,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    let mut task = load_error_task(db, &command.action.item_id, executor).await?;
    ensure_error_task_subject(&task, &command.expected_subject_version)?;
    let mut fact = error_action_fact(db, &task, &command.action, actor_id, executor).await?;
    let summary = error_action_summary(&command.action, &fact)?;
    task.record_attempt(Instant::now(), Some(summary))?;
    db.integration_error_tasks().update(&mut task, executor).await?;
    fact.next_subject_version = Some(task.base.version.to_string());
    Ok(fact)
}

async fn error_action_fact(
    db: &Database,
    task: &IntegrationErrorTask,
    action: &IntegrationNonTerminalTaskAction,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    let subject = EvidenceSubject::error(task);
    match action.kind {
        IntegrationTaskActionKind::QueryOriginalResult => query_action_fact(db, &subject, executor).await,
        IntegrationTaskActionKind::AddEvidence => {
            let verified =
                verify_evidence_refs(db, &subject, &action.evidence_refs, actor_id, executor).await?;
            Ok(ActionFact {
                outcome: IntegrationActionOutcome::EvidenceAdded,
                business_result_reference: Some(verified_reference(&verified)?),
                next_subject_version: None,
                verified_evidence: verified.into_iter().map(|evidence| evidence.reference).collect(),
            })
        }
        IntegrationTaskActionKind::ReplayOriginal => {
            if !task.can_replay_original() {
                return Err(Error::BusinessLogicError(
                    "必须先由服务端查询并确认原动作无结果，且错误分类允许重放".to_string(),
                ));
            }
            let reference = db.replay_original(&subject, executor).await?;
            Ok(ActionFact {
                outcome: IntegrationActionOutcome::ReplayAccepted,
                business_result_reference: Some(reference),
                next_subject_version: None,
                verified_evidence: Vec::new(),
            })
        }
        IntegrationTaskActionKind::Reattribute => {
            let reference = db.verify_reattribution(&subject, executor).await?;
            Ok(ActionFact {
                outcome: IntegrationActionOutcome::Reattributed,
                business_result_reference: Some(reference),
                next_subject_version: None,
                verified_evidence: db.discover_evidence(&subject, executor).await?,
            })
        }
        IntegrationTaskActionKind::LinkCompensation => {
            if !action
                .evidence_refs
                .iter()
                .any(|evidence| evidence.kind == ControlledEvidenceKind::CompensationResult)
            {
                return Err(Error::ValidationError("关联补偿必须提供补偿结果证据".to_string()));
            }
            let verified =
                verify_evidence_refs(db, &subject, &action.evidence_refs, actor_id, executor).await?;
            Ok(ActionFact {
                outcome: IntegrationActionOutcome::EvidenceLinked,
                business_result_reference: Some(verified_reference(&verified)?),
                next_subject_version: None,
                verified_evidence: verified.into_iter().map(|evidence| evidence.reference).collect(),
            })
        }
    }
}

async fn query_action_fact(
    db: &Database,
    subject: &EvidenceSubject,
    executor: &mut dyn Executor,
) -> Result<ActionFact> {
    match db.query_original(subject, executor).await? {
        OriginalResultFact::Terminal(reference) => Ok(ActionFact {
            outcome: IntegrationActionOutcome::TerminalEvidenceFound,
            business_result_reference: Some(reference),
            next_subject_version: None,
            verified_evidence: db.discover_evidence(subject, executor).await?,
        }),
        OriginalResultFact::NoResult => Ok(ActionFact {
            outcome: IntegrationActionOutcome::NoResultConfirmed,
            business_result_reference: None,
            next_subject_version: None,
            verified_evidence: Vec::new(),
        }),
        OriginalResultFact::Unknown => Ok(unknown_action_fact()),
    }
}

fn unknown_action_fact() -> ActionFact {
    ActionFact {
        outcome: IntegrationActionOutcome::ResultUnknown,
        business_result_reference: None,
        next_subject_version: None,
        verified_evidence: Vec::new(),
    }
}

fn error_action_summary(action: &IntegrationNonTerminalTaskAction, fact: &ActionFact) -> Result<String> {
    let evidence = compact_evidence(&action.evidence_refs)?;
    let summary = format!(
        "w29_action={};operation={};outcome={:?};evidence={}",
        action.kind.as_str(),
        action.operation_id,
        fact.outcome,
        evidence.unwrap_or_else(|| "none".to_string())
    );
    if summary.len() > 512 {
        return Err(Error::ValidationError("动作证据摘要过长".to_string()));
    }
    Ok(summary)
}

async fn execute_difference_task_action(
    db: &Database,
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
        difference_action_fact(db, &difference, &command.action, receipt_id, actor_id, executor).await?;
    let record = append_resolution(&difference, latest.as_ref(), &fact, receipt_id, actor_id)?;
    let next_subject_version = record.resolution_no.to_string();
    db.reconciliation_difference_resolutions()
        .create(&record, executor)
        .await?;
    Ok(ActionFact {
        outcome: fact.outcome,
        business_result_reference: fact.business_result_reference,
        next_subject_version: Some(next_subject_version),
        verified_evidence: fact.verified_evidence,
    })
}

pub(super) async fn difference_action_fact(
    db: &Database,
    difference: &ReconciliationDifference,
    action: &IntegrationNonTerminalTaskAction,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    let subject = EvidenceSubject::difference(difference);
    match action.kind {
        IntegrationTaskActionKind::QueryOriginalResult => {
            let fact = query_action_fact(db, &subject, executor).await?;
            Ok(DirectFact {
                action: ResolutionAction::QueryOriginalResult,
                evidence_reference: Some(audit_log_reference(receipt_id)?),
                resulting_status: DirectReconciliationStatus::Open,
                outcome: fact.outcome,
                business_result_reference: fact.business_result_reference,
                verified_evidence: fact.verified_evidence,
            })
        }
        IntegrationTaskActionKind::AddEvidence => {
            let verified =
                verify_evidence_refs(db, &subject, &action.evidence_refs, actor_id, executor).await?;
            let evidence = verified_reference(&verified)?;
            Ok(DirectFact {
                action: ResolutionAction::AddEvidence,
                evidence_reference: Some(evidence),
                resulting_status: DirectReconciliationStatus::EvidencePending,
                outcome: IntegrationActionOutcome::EvidenceAdded,
                business_result_reference: None,
                verified_evidence: verified.into_iter().map(|evidence| evidence.reference).collect(),
            })
        }
        IntegrationTaskActionKind::ReplayOriginal => {
            let reference = db.replay_original(&subject, executor).await?;
            Ok(DirectFact {
                action: ResolutionAction::ReplayOriginal,
                evidence_reference: None,
                resulting_status: DirectReconciliationStatus::Open,
                outcome: IntegrationActionOutcome::ReplayAccepted,
                business_result_reference: Some(reference),
                verified_evidence: Vec::new(),
            })
        }
        IntegrationTaskActionKind::Reattribute => {
            let reference = db.verify_reattribution(&subject, executor).await?;
            Ok(DirectFact {
                action: ResolutionAction::Reattribute,
                evidence_reference: Some(reference.clone()),
                resulting_status: DirectReconciliationStatus::EvidencePending,
                outcome: IntegrationActionOutcome::Reattributed,
                business_result_reference: Some(reference),
                verified_evidence: db.discover_evidence(&subject, executor).await?,
            })
        }
        IntegrationTaskActionKind::LinkCompensation => {
            if !action
                .evidence_refs
                .iter()
                .any(|evidence| evidence.kind == ControlledEvidenceKind::CompensationResult)
            {
                return Err(Error::ValidationError("关联补偿必须提供补偿结果证据".to_string()));
            }
            let verified =
                verify_evidence_refs(db, &subject, &action.evidence_refs, actor_id, executor).await?;
            let reference = verified_reference(&verified)?;
            Ok(DirectFact {
                action: ResolutionAction::LinkCompensation,
                evidence_reference: Some(reference.clone()),
                resulting_status: DirectReconciliationStatus::EvidencePending,
                outcome: IntegrationActionOutcome::EvidenceLinked,
                business_result_reference: Some(reference),
                verified_evidence: verified.into_iter().map(|evidence| evidence.reference).collect(),
            })
        }
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

/// 由收据 ID 构造 `audit_log:id` 证据记录引用。
///
/// # 参数
/// * `receipt_id` - 命令收据 ID
///
/// # 返回
/// 返回精确 `type:id` 编码。
///
/// # 错误
/// 收据 ID 不符合证据记录 grammar 时返回校验错误。
///
/// # 约束
/// 语法由 [`EvidenceRecordRef`] 独占，禁止 `format!` 拼接。
fn audit_log_reference(receipt_id: &str) -> Result<String> {
    EvidenceRecordRef::new("audit_log", receipt_id)
        .map(|reference| reference.to_string())
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 推导单次动作后的下一开放动作代码（推导规则归领域，此处只做代码映射）。
///
/// # 参数
/// * `item_type` - 业务项类型
/// * `outcome` - 本次动作结果
///
/// # 返回
/// 返回下一开放动作代码。
pub(super) fn next_allowed_actions(
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
    next_actions_after_outcome(subject, projected)
        .iter()
        .map(|action| action.as_str().to_string())
        .collect()
}

/// 将动作证据引用规范化为可写入摘要的紧凑集合。
///
/// # 参数
/// * `refs` - 客户端提交的受控证据引用
///
/// # 返回
/// 空集合返回 `Ok(None)`；否则返回排序去重后的紧凑编码。
///
/// # 错误
/// 记录 ID 非法或编码超过 512 字节时返回校验错误。
///
/// # 约束
/// grammar、排序、去重与长度由 [`CompactEvidenceSet`] 独占。
fn compact_evidence(refs: &[ControlledEvidenceRef]) -> Result<Option<String>> {
    CompactEvidenceSet::try_from_pairs(
        refs.iter()
            .map(|evidence| (evidence.kind.as_str(), evidence.record_id.as_str())),
    )
    .map(|set| set.map(CompactEvidenceSet::into_wire))
    .map_err(|error| Error::ValidationError(error.to_string()))
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
