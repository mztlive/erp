//! W29 强类型任务动作、任务完成与无任务直接对账决定。

use database::{AccessControlExt, Executor, IntegrationOpsExt, NoTransaction, WorkItemExt};
use entities::common::time::Instant;
use entities::integration_ops::{
    error_work_item_type, next_actions_after_outcome, CompactEvidenceSet, DirectConclusion, ErrorTaskStatus,
    EvidenceRecordRef, IntegrationCommandIdentity, IntegrationErrorTask, ProjectionOutcome,
    ProjectionSubject, ReconciliationDifference, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionId, ResolutionAction,
    ResolutionType, ResolutionVersionCheck,
};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use mongodb::Database;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::evidence::{
    difference_evidence_policy, ensure_completion_policy, ensure_direct_reason, error_evidence_policy,
    verified_reference, verify_evidence_refs, EvidenceSubject, IntegrationEvidenceAuthority,
    OriginalResultFact,
};
use super::{
    ControlledEvidenceRef, DirectReconciliationCommand, DirectReconciliationDecision,
    DirectReconciliationResult, DirectReconciliationStatus, IntegrationActionOutcome, IntegrationItemType,
    IntegrationNonTerminalTaskAction, IntegrationOpsService, IntegrationTaskActionCommand,
    IntegrationTaskActionEvidence, IntegrationTaskActionKind, IntegrationTaskActionResult,
    IntegrationTaskCompletionCommand, IntegrationTaskCompletionResult, IntegrationWorkItemStatus,
    PreparedDirectDecisionTarget, PreparedWorkItemTarget,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;

const TASK_ACTION_AUDIT: &str = "integration.task_action";
const TASK_COMPLETION_AUDIT: &str = "integration.task_completion";
const DIRECT_DECISION_AUDIT: &str = "integration.direct_reconciliation";

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
        if let Some(result) = self.replay_direct_decision(&receipt, &command, actor).await? {
            return Ok(result);
        }
        let result = self
            .transact_direct_decision(command.clone(), actor.clone(), receipt.clone())
            .await;
        self.recover_direct_decision(result, &receipt, &command, actor)
            .await
    }

    async fn transact_task_action(
        &self,
        command: IntegrationTaskActionCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<IntegrationTaskActionResult> {
        let rbac = crate::iam::shared_rbac_service(self.db.clone());
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

    async fn transact_task_completion(
        &self,
        command: IntegrationTaskCompletionCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<IntegrationTaskCompletionResult> {
        let rbac = crate::iam::shared_rbac_service(self.db.clone());
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

    async fn transact_direct_decision(
        &self,
        command: DirectReconciliationCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<DirectReconciliationResult> {
        self.run_audited(move |db, session| {
            Box::pin(async move {
                let prepared = PreparedDirectDecisionTarget::try_from(&command)?;
                ensure_no_work_item(db, &prepared.difference_id, session).await?;
                let difference = load_difference(db, &prepared.difference_id, session).await?;
                let latest = latest_resolution(db, &prepared.difference_id, session).await?;
                ensure_direct_version(prepared.difference_version, latest.as_ref())?;
                ensure_difference_open(latest.as_ref())?;
                let fact = direct_decision_fact(
                    db,
                    &difference,
                    &command,
                    receipt.receipt_id(),
                    actor.id(),
                    session,
                )
                .await?;
                let record = append_resolution(
                    &difference,
                    latest.as_ref(),
                    &fact,
                    receipt.receipt_id(),
                    actor.id(),
                )?;
                db.reconciliation_difference_resolutions()
                    .create(&record, session)
                    .await?;
                store_direct_receipt(db, &actor, &receipt, &fact, session).await?;
                Ok(direct_result(&command, receipt.receipt_id(), fact))
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

    async fn replay_direct_decision(
        &self,
        receipt: &IntegrationCommandIdentity,
        command: &DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<Option<DirectReconciliationResult>> {
        let Some(message) = self
            .replay_receipt::<DirectReceiptMessage>(receipt, actor)
            .await?
        else {
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

    async fn replay_receipt<T: DeserializeOwned>(
        &self,
        receipt: &IntegrationCommandIdentity,
        actor: &AuditActor,
    ) -> Result<Option<T>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(receipt.receipt_id(), &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if !receipt.matches_receipt(
            &audit.actor_id,
            &audit.action,
            &audit.resource_type,
            audit.resource_id.as_deref(),
        ) || actor.id() != audit.actor_id
        {
            return Err(Error::ConflictError("幂等键已用于不同命令".to_string()));
        }
        let message = audit
            .message
            .as_deref()
            .ok_or_else(|| Error::Internal("W29 幂等收据缺少结果".to_string()))?;
        let envelope: ReceiptEnvelope<T> =
            serde_json::from_str(message).map_err(|_| Error::Internal("W29 幂等收据不可解析".to_string()))?;
        if envelope.fingerprint != receipt.fingerprint() {
            return Err(Error::ConflictError("幂等键已用于不同命令".to_string()));
        }
        Ok(Some(envelope.result))
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

    async fn recover_direct_decision(
        &self,
        result: Result<DirectReconciliationResult>,
        receipt: &IntegrationCommandIdentity,
        command: &DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<DirectReconciliationResult> {
        match result {
            Ok(result) => Ok(result),
            Err(error) => match self.replay_direct_decision(receipt, command, actor).await? {
                Some(result) => Ok(result),
                None => Err(error),
            },
        }
    }
}

/// 序列化 W29 命令并构造领域幂等身份。
///
/// # 参数
/// * `actor_id` - 命令操作人
/// * `action` - 稳定动作名
/// * `resource_type` - 审计资源类型
/// * `resource_id` - 审计资源 ID
/// * `idempotency_key` - 客户端幂等键
/// * `command` - 完整强命令载荷
///
/// # 返回
/// 返回不暴露原始幂等键的领域命令身份。
///
/// # 错误
/// 命令无法序列化时返回内部错误。
fn command_identity<T: Serialize>(
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    idempotency_key: &str,
    command: &T,
) -> Result<IntegrationCommandIdentity> {
    let payload =
        serde_json::to_vec(command).map_err(|_| Error::Internal("W29 命令无法形成幂等指纹".to_string()))?;
    Ok(IntegrationCommandIdentity::new(
        actor_id,
        action,
        resource_type,
        resource_id,
        idempotency_key,
        &payload,
    ))
}

#[derive(Debug, Clone)]
struct ActionFact {
    outcome: IntegrationActionOutcome,
    business_result_reference: Option<String>,
    next_subject_version: Option<String>,
    verified_evidence: Vec<ControlledEvidenceRef>,
}

#[derive(Debug)]
struct TerminalFact {
    reference: String,
    next_subject_version: String,
}

#[derive(Debug, Clone)]
struct DirectFact {
    action: ResolutionAction,
    evidence_reference: Option<String>,
    resulting_status: DirectReconciliationStatus,
    outcome: IntegrationActionOutcome,
    business_result_reference: Option<String>,
    verified_evidence: Vec<ControlledEvidenceRef>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptEnvelope<T> {
    #[serde(rename = "f")]
    fingerprint: String,
    #[serde(rename = "r")]
    result: T,
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

#[derive(Debug, Serialize, Deserialize)]
struct CompletionReceiptMessage {
    #[serde(rename = "e")]
    terminal_evidence_reference: String,
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

/// 加载与命令绑定的正式任务，并校验责任归属与版本。
///
/// 任务版本与业务主题版本取自 Prepared 目标（DTO 层单次解析），此处只做
/// typed 比较，不再解析版本字符串。
async fn load_bound_work_item(
    db: &Database,
    target: &PreparedWorkItemTarget,
    action: &IntegrationNonTerminalTaskAction,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let item = db
        .work_items()
        .find_by_id(&target.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("正式任务不存在".to_string()))?;
    ensure_work_item_version(&item, target.task_version, &target.subject_version)?;
    ensure_work_item_responsibility(&item, actor_id)?;
    ensure_actor_eligible(db, &item, actor_id, executor).await?;
    ensure_work_item_association(db, &item, action, executor).await?;
    Ok(item)
}

/// 校验任务与业务主题版本与调用方冻结的 typed 版本一致（无二次解析）。
fn ensure_work_item_version(item: &WorkItem, task_version: u64, subject_version: &str) -> Result<()> {
    if item.base.version != task_version || item.subject_version != subject_version {
        return Err(Error::ConflictError(
            "任务或业务主题版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

fn ensure_work_item_responsibility(item: &WorkItem, actor_id: &str) -> Result<()> {
    if item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("任务已不再开放".to_string()));
    }
    if !item.is_owned_by(actor_id) {
        return Err(Error::Forbidden("当前账号不是任务的当前责任人".to_string()));
    }
    if false {
        return Err(Error::BusinessLogicError("W29 只接受独立异常任务".to_string()));
    }
    Ok(())
}

async fn ensure_actor_eligible(
    db: &Database,
    item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, item, actor_id, executor);
    Ok(())
}

async fn ensure_work_item_association(
    db: &Database,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    let expected = match action.item_type {
        IntegrationItemType::ErrorTask => {
            let task = db
                .integration_error_tasks()
                .find_by_id(&action.item_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("集成错误任务不存在".to_string()))?;
            (error_work_item_type(task.error_class), "integration_error_task")
        }
        IntegrationItemType::ReconciliationDifference => {
            (WorkItemType::BusinessException, "reconciliation_difference")
        }
    };
    if item.work_item_type != expected.0
        || item.business_object_type != expected.1
        || item.business_object_id != action.item_id
    {
        return Err(Error::ConflictError("任务与业务项的正式关联不一致".to_string()));
    }
    Ok(())
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
                .any(|evidence| evidence.kind == super::ControlledEvidenceKind::CompensationResult)
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

async fn difference_action_fact(
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
                .any(|evidence| evidence.kind == super::ControlledEvidenceKind::CompensationResult)
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
            .any(|evidence| evidence.reference.kind == super::ControlledEvidenceKind::CompensationResult),
        verified.iter().any(|evidence| {
            evidence.reference.kind == super::ControlledEvidenceKind::BusinessObjectVerification
        }),
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

async fn ensure_no_work_item(db: &Database, difference_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let items = db
        .work_items()
        .find_unique_for_reconciliation_difference(difference_id, executor)
        .await?;
    if items.is_empty() {
        return Ok(());
    }
    Err(Error::ConflictError(
        "差异已关联正式任务，必须通过 W29 任务强命令处理".to_string(),
    ))
}

async fn load_error_task(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<IntegrationErrorTask> {
    let task = db
        .integration_error_tasks()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("集成错误任务不存在".to_string()))?;
    if task.is_terminal() {
        return Err(Error::ConflictError("集成错误任务已终结".to_string()));
    }
    Ok(task)
}

fn ensure_error_task_subject(task: &IntegrationErrorTask, expected: &str) -> Result<()> {
    if !task.has_subject_version(expected) {
        return Err(Error::ConflictError("错误任务业务版本已变化".to_string()));
    }
    Ok(())
}

async fn load_difference(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<ReconciliationDifference> {
    db.reconciliation_differences()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("对账差异不存在".to_string()))
}

fn ensure_difference_subject(
    latest: Option<&ReconciliationDifferenceResolution>,
    expected: &str,
) -> Result<()> {
    match ReconciliationDifferenceResolution::check_version(expected, latest) {
        ResolutionVersionCheck::Current => Ok(()),
        ResolutionVersionCheck::Invalid | ResolutionVersionCheck::Stale => {
            Err(Error::ConflictError("对账差异业务版本已变化".to_string()))
        }
    }
}

async fn latest_resolution(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ReconciliationDifferenceResolution>> {
    db.reconciliation_difference_resolutions()
        .find_latest_by_difference(&ReconciliationDifferenceId::new(id.to_string()), executor)
        .await
        .map_err(Into::into)
}

fn ensure_difference_open(latest: Option<&ReconciliationDifferenceResolution>) -> Result<()> {
    if !ReconciliationDifferenceResolution::is_open(latest) {
        return Err(Error::ConflictError("对账差异已形成正式结论".to_string()));
    }
    Ok(())
}

/// 校验直接对账差异版本与当前决定序号一致（typed 比较，无二次解析）。
///
/// 非十进制输入已由 Prepared 目标在 DTO 层拒绝；此处只区分当前与陈旧。
fn ensure_direct_version(expected: u64, latest: Option<&ReconciliationDifferenceResolution>) -> Result<()> {
    if ReconciliationDifferenceResolution::current_version(latest) == expected {
        Ok(())
    } else {
        Err(Error::ConflictError(
            "差异决定版本已变化，请刷新后重试".to_string(),
        ))
    }
}

async fn direct_decision_fact(
    db: &Database,
    difference: &ReconciliationDifference,
    command: &DirectReconciliationCommand,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    match &command.decision {
        DirectReconciliationDecision::NonTerminalAction {
            action,
            evidence_refs,
            comment: _,
        } => {
            difference_action_fact(
                db,
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
        }
        DirectReconciliationDecision::TerminalConclusion {
            conclusion,
            reason_code,
            reason_registry_id,
            reason_registry_version,
            registered_reason_id,
            evidence_refs,
            comment: _,
        } => {
            ensure_direct_reason(
                reason_registry_id,
                *reason_registry_version,
                registered_reason_id,
                *reason_code,
                *conclusion,
                evidence_refs,
            )?;
            let subject = EvidenceSubject::difference(difference);
            let verified = verify_evidence_refs(db, &subject, evidence_refs, actor_id, executor).await?;
            let reference = verified_reference(&verified)?;
            let typed = DirectConclusion::from(*conclusion);
            let (resulting_status, outcome) = match conclusion {
                super::DirectReconciliationConclusion::ConfirmNoError => (
                    DirectReconciliationStatus::ConfirmedNoError,
                    IntegrationActionOutcome::ConfirmedNoError,
                ),
                super::DirectReconciliationConclusion::ConfirmValidDifference => (
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
    }
}

/// 经领域追加工厂形成决定记录（序号递增与状态派生归领域，时间与身份由调用方注入）。
///
/// # 参数
/// * `difference` - 所属对账差异
/// * `latest` - 当前最新决定；`None` 表示首条追加
/// * `fact` - 已验证的决定事实（动作与证据引用）
/// * `record_id` - 记录主键（调用方收据 ID）
/// * `actor_id` - 决定人
///
/// # 返回
/// 返回新建的不可变追加式决定记录。
///
/// # 错误
/// 序号已达上限时返回 `ConflictError`，其余领域约束失败保持领域逻辑错误。
///
/// # 约束
/// 序号上限文案由领域拥有，此处只做错误类别映射，不维护第二份上限规则。
fn append_resolution(
    difference: &ReconciliationDifference,
    latest: Option<&ReconciliationDifferenceResolution>,
    fact: &DirectFact,
    record_id: &str,
    actor_id: &str,
) -> Result<ReconciliationDifferenceResolution> {
    ReconciliationDifferenceResolution::append(
        ReconciliationDifferenceResolutionId::new(record_id.to_string()),
        ReconciliationDifferenceId::new(difference.base.id.clone()),
        latest,
        fact.action,
        fact.evidence_reference.clone(),
        actor_id.to_string(),
        Instant::now(),
    )
    .map_err(|error| {
        let message = error.to_string();
        if message == "差异决定序号已达上限" {
            Error::ConflictError(message)
        } else {
            Error::Logic(error)
        }
    })
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
fn next_allowed_actions(item_type: IntegrationItemType, outcome: IntegrationActionOutcome) -> Vec<String> {
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

async fn store_receipt<T: Serialize>(
    db: &Database,
    actor: &AuditActor,
    receipt: &IntegrationCommandIdentity,
    result: T,
    executor: &mut dyn Executor,
) -> Result<()> {
    let message = serde_json::to_string(&ReceiptEnvelope {
        fingerprint: receipt.fingerprint().to_string(),
        result,
    })
    .map_err(|_| Error::Internal("W29 结果无法形成幂等收据".to_string()))?;
    let audit = actor.clone().resource_log_with_id(
        receipt.receipt_id().to_string(),
        receipt.action(),
        receipt.resource_type(),
        receipt.resource_id().to_string(),
        Some(message),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{command_identity, next_allowed_actions};
    use crate::integration_ops::{
        IntegrationActionOutcome, IntegrationItemType, IntegrationNonTerminalTaskAction,
        IntegrationTaskActionCommand, IntegrationTaskActionKind,
    };

    fn command(key: &str, kind: IntegrationTaskActionKind) -> IntegrationTaskActionCommand {
        IntegrationTaskActionCommand {
            work_item_id: "wi-1".to_string(),
            expected_task_version: "1".to_string(),
            expected_subject_version: "1".to_string(),
            action: IntegrationNonTerminalTaskAction {
                item_type: IntegrationItemType::ErrorTask,
                item_id: "task-1".to_string(),
                kind,
                operation_id: "op-1".to_string(),
                reason_code: None,
                comment: None,
                evidence_refs: Vec::new(),
            },
            idempotency_key: key.to_string(),
        }
    }

    #[test]
    fn receipt_never_contains_raw_idempotency_key() {
        let command = command("raw-secret-key", IntegrationTaskActionKind::QueryOriginalResult);
        let receipt = command_identity(
            "actor-1",
            super::TASK_ACTION_AUDIT,
            "work_item",
            "wi-1",
            &command.idempotency_key,
            &command,
        )
        .unwrap();

        assert!(!receipt.receipt_id().contains("raw-secret-key"));
        assert_eq!(receipt.resource_id(), "wi-1");
        assert_eq!(receipt.fingerprint().len(), 64);
    }

    #[test]
    fn terminal_evidence_allows_explicit_resolve_without_completing_action() {
        let actions = next_allowed_actions(
            IntegrationItemType::ErrorTask,
            IntegrationActionOutcome::TerminalEvidenceFound,
        );
        assert!(actions.iter().any(|action| action == "RESOLVE"));

        let actions = next_allowed_actions(
            IntegrationItemType::ReconciliationDifference,
            IntegrationActionOutcome::TerminalEvidenceFound,
        );
        assert!(actions.iter().any(|action| action == "RESOLVE"));
    }

    #[test]
    fn confirmed_no_result_allows_replay_only_for_error_task() {
        let actions = next_allowed_actions(
            IntegrationItemType::ErrorTask,
            IntegrationActionOutcome::NoResultConfirmed,
        );
        assert!(actions.iter().any(|action| action == "REPLAY_ORIGINAL"));

        let actions = next_allowed_actions(
            IntegrationItemType::ReconciliationDifference,
            IntegrationActionOutcome::NoResultConfirmed,
        );
        assert!(!actions.iter().any(|action| action == "REPLAY_ORIGINAL"));
    }

    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("task_decision.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 分层守卫（INT-E22）：版本只由 Prepared DTO 单次解析，服务只做 typed 比较。
    ///
    /// 锁定服务不再对版本字符串二次解析；typed 版本与规范化字段来自 Prepared 目标。
    #[test]
    fn versions_flow_typed_from_prepared_without_reparse() {
        let source = production_source();
        assert!(!source.contains("parse::<u64>()"));
        assert!(source.contains("PreparedWorkItemTarget::try_from(&command)"));
        assert!(source.contains("PreparedDirectDecisionTarget::try_from(&command)"));
        assert!(source.contains("target.task_version"));
        assert!(source.contains("prepared.difference_version"));
    }

    /// 分层守卫（INT-E23）：结论映射与序号追加归领域，服务只做编排与结果映射。
    ///
    /// 锁定旧派生源（`completion_as_action`、`build_resolution`、结论三元组）
    /// 已删除；动作/状态派生走领域结论映射与追加工厂。
    #[test]
    fn conclusion_and_append_are_owned_by_domain() {
        let source = production_source();
        assert!(!source.contains("fn completion_as_action"));
        assert!(!source.contains("fn build_resolution"));
        assert!(!source.contains("ConfirmValidDifference,\n"));
        assert!(source.contains("DirectConclusion::from("));
        assert!(source.contains("ReconciliationDifferenceResolution::append("));
        assert!(source.contains("as_non_terminal_action()"));
    }
}
