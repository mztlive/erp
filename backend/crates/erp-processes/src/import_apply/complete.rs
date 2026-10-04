use application_core::{AuditActor, StructuredCommandReceipt};
use erp_audit::BusinessEventContext;
use erp_core::common::time::Instant;
pub(super) use erp_import::entity::command_receipt::ConfirmationCompletionReceipt;
use erp_import::entity::command_receipt::{
    ImportCommandReceipt, ImportCommandResult, ImportConfirmationOutcome,
};
use erp_import::repository::ImportCommandReceiptExt;
use erp_import::repository::prelude::*;
use erp_import::{
    CompleteImportBusinessConfirmationCommand, ConfirmationDecision, ConfirmationScope, ConfirmationStatus,
    ImportBusinessConfirmationNextStep, ImportBusinessConfirmationResultStatus, LegacyImportBatch,
    LegacyImportBatchStatus, LegacyImportCommandIdentity, LegacyImportConfirmation, LegacyImportExt,
    PreparedConfirmationCompletion,
};
use erp_workflow::entity::document_registry::WorkflowActionId;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::{DocumentRegistryExt, WorkItemExt};
use id_generator::next_id;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::command_event::{import_event_content, import_event_context};
use super::confirmation_query::{confirmation_view, work_item_view};
use super::create_confirmation::{confirmation_next_step, replace_confirmation_in_matrix};
use super::dto::CompleteImportBusinessConfirmationResult;
use super::{
    IMPORT_CONFIRMATION_COMMAND_PREFIX, IMPORT_CONFIRMATION_OBJECT_TYPE, IMPORT_CONFIRMATION_ORGANIZATION,
    ImportApplyService,
};
use crate::adapters::workflow::work_item_service;
use crate::audit::{AuditedCommand, AuditedWrite, MongoAuditEventSink, execute_audited};
use crate::{Error, Result};

impl ImportApplyService {
    /// 执行确认完成命令，正式事实、任务、独立回执与成功事件原子提交。
    /// # 参数
    /// * `req` - 已认证入口传入的强类型命令。
    /// * `actor` - 当前操作人。
    /// # 返回
    /// 返回原确认结果、任务和事件关联号。
    /// # 错误
    /// 沿用任务责任、授权、版本和幂等冲突；损坏回执不可恢复。
    pub async fn complete_import_business_confirmation(
        &self,
        req: CompleteImportBusinessConfirmationCommand,
        actor: &AuditActor,
    ) -> Result<CompleteImportBusinessConfirmationResult> {
        req.validate()?;
        let prepared = PreparedConfirmationCompletion::try_from(req)?;
        let identity =
            confirmation_command_identity(actor.id(), "legacy_import_confirmation.complete", &prepared)
                .structured_receipt("legacy_import_confirmation")?;
        if let Some(result) = self.replay_confirmation_completion(&identity, &prepared, actor).await? {
            return Ok(result);
        }
        let context = import_event_context(
            actor,
            "legacy_import_confirmation.complete",
            "legacy_import_confirmation",
            "完成导入业务确认",
            &identity,
        )?;
        match self.commit_confirmation_completion(&identity, &prepared, actor, &context).await {
            Ok(result) => Ok(result),
            Err(error) => crate::audit::recover_command(
                error,
                self.replay_confirmation_completion(&identity, &prepared, actor).await,
            ),
        }
    }

    /// 用同一事务执行器读取回执、重验当前责任并恢复原结果。
    async fn replay_confirmation_completion(
        &self,
        identity: &StructuredCommandReceipt,
        prepared: &PreparedConfirmationCompletion,
        actor: &AuditActor,
    ) -> Result<Option<CompleteImportBusinessConfirmationResult>> {
        let db = self.db.clone();
        let identity = identity.clone();
        let prepared = prepared.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replay_confirmation_step(&db, &identity, &prepared, &actor, executor).await
                })
            })
            .await
    }

    /// 保持原请求生命周期，在已有事务内执行类型化业务边界。
    async fn commit_confirmation_completion(
        &self,
        identity: &StructuredCommandReceipt,
        prepared: &PreparedConfirmationCompletion,
        actor: &AuditActor,
        context: &BusinessEventContext,
    ) -> Result<CompleteImportBusinessConfirmationResult> {
        let db = self.db.clone();
        let identity = identity.clone();
        let prepared = prepared.clone();
        let actor = actor.clone();
        let context = context.clone();
        let decided_at = Instant::now();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let command = ConfirmationWrite {
                        db: &db,
                        identity: &identity,
                        prepared: &prepared,
                        actor: &actor,
                        event_id: context.event_id(),
                        at: decided_at,
                    };
                    execute_audited(&context, &MongoAuditEventSink::new(&db), executor, &command).await
                })
            })
            .await
    }
}

/// 导入确认的类型化正式命令边界。
struct ConfirmationWrite<'a> {
    db: &'a mongodb::Database,
    identity: &'a StructuredCommandReceipt,
    prepared: &'a PreparedConfirmationCompletion,
    actor: &'a AuditActor,
    event_id: &'a str,
    at: Instant,
}

#[async_trait::async_trait]
impl AuditedCommand for ConfirmationWrite<'_> {
    type Output = CompleteImportBusinessConfirmationResult;

    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        if let Some(result) =
            replay_confirmation_step(self.db, self.identity, self.prepared, self.actor, executor).await?
        {
            return Ok(AuditedWrite::Replayed(result));
        }
        let source = load_confirmation_source(self, executor).await?;
        let result = apply_confirmation_completion(self, source, executor).await?;
        Ok(AuditedWrite::Fresh { content: import_event_content(result.confirmation.id.clone()), result })
    }
}

struct ConfirmationSource {
    confirmation: LegacyImportConfirmation,
    work_item: WorkItem,
    batch: LegacyImportBatch,
}

/// 原任务、确认和批次版本校验完成后重验当前责任资格。
async fn load_confirmation_source(
    command: &ConfirmationWrite<'_>,
    executor: &mut dyn Executor,
) -> Result<ConfirmationSource> {
    let db = command.db;
    let work_item = db
        .work_items()
        .find_by_id(command.prepared.work_item_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("导入确认任务不存在".into()))?;
    let confirmation = db
        .legacy_import_confirmations()
        .find_by_work_item(&command.prepared.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("导入确认事实不存在".into()))?;
    let batch = db
        .legacy_import_batches()
        .find_by_id(confirmation.batch_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("导入批次不存在".into()))?;
    validate_confirmation_completion(
        command.prepared,
        &work_item,
        &confirmation,
        &batch,
        command.actor.id(),
    )?;
    work_item_service(db.clone(), crate::adapters::identity::shared_rbac_service(db.clone()))
        .ensure_domain_decision_access(command.actor, &work_item, executor)
        .await?;
    Ok(ConfirmationSource { confirmation, work_item, batch })
}

/// 按原确认矩阵和状态规则形成内存变更，再统一持久化。
async fn apply_confirmation_completion(
    command: &ConfirmationWrite<'_>,
    mut source: ConfirmationSource,
    executor: &mut dyn Executor,
) -> Result<CompleteImportBusinessConfirmationResult> {
    let prepared = command.prepared;
    let mut matrix = command
        .db
        .legacy_import_confirmations()
        .list_by_batch(&source.confirmation.batch_id, executor)
        .await?;
    source.confirmation.decide(
        prepared.decision,
        command.actor.id().to_string(),
        command.at,
        prepared.reason_code.clone(),
        prepared.comment.clone(),
    )?;
    source.work_item.record_activity(command.actor.id(), command.at)?;
    source.work_item.complete_by_domain_command(command.actor.id().to_string(), command.at)?;
    replace_confirmation_in_matrix(&mut matrix, &source.confirmation);
    let current = LegacyImportConfirmation::current_matrix(
        &matrix,
        source.confirmation.batch_version,
        source.confirmation.trial_version,
        &source.confirmation.import_rule_version,
    );
    let next_step = confirmation_next_step(LegacyImportConfirmation::matrix_decision(
        prepared.decision,
        &current,
        &source.batch.required_confirmation_scopes()?,
    ));
    source.batch.update_summaries(
        source.batch.failure_code_summary.clone(),
        Some(LegacyImportConfirmation::matrix_summary(source.confirmation.trial_version, &current)),
    )?;
    if next_step == ImportBusinessConfirmationNextStep::StartApply {
        source.batch.advance(LegacyImportBatchStatus::ReadyToApply)?;
    }
    persist_confirmation_result(command, source, next_step, executor).await
}

/// 事实、任务、动作及独立回执使用唯一执行器；成功事件由外层边界写入。
async fn persist_confirmation_result(
    command: &ConfirmationWrite<'_>,
    mut source: ConfirmationSource,
    next_step: ImportBusinessConfirmationNextStep,
    executor: &mut dyn Executor,
) -> Result<CompleteImportBusinessConfirmationResult> {
    let db = command.db;
    let workflow_action = super::factories::confirmation_workflow_action(
        WorkflowActionId::new(next_id()),
        &source.confirmation,
        command.actor.id(),
    )?;
    db.legacy_import_confirmations().update(&mut source.confirmation, executor).await?;
    db.legacy_import_batches().update(&mut source.batch, executor).await?;
    db.work_items().update(&mut source.work_item, executor).await?;
    db.workflow_actions().create(&workflow_action, executor).await?;
    let receipt = ConfirmationCompletionReceipt {
        result_status: confirmation_result_status(command.prepared.decision),
        task_version: source.work_item.base.version,
        batch_version: source.batch.base.version,
        next_step,
    };
    let outcome = ImportConfirmationOutcome {
        confirmation_id: source.confirmation.base.id.clone(),
        confirmation_version: source.confirmation.base.version,
        batch_id: source.batch.base.id,
        work_item_id: source.work_item.base.id.clone(),
        subject_version: source.work_item.subject_version.clone(),
        confirmation_scope: source.confirmation.confirmation_scope.clone(),
        decision: command.prepared.decision,
        decided_at: command.at,
        receipt,
    };
    let fact = ImportCommandReceipt::new(
        command.identity.clone(),
        ImportCommandResult::Confirmation(Box::new(outcome)),
        command.event_id.to_string(),
    )?;
    db.import_command_receipts().create(&fact, executor).await?;
    Ok(completion_result(
        ConfirmationCompletionTransactionResult {
            confirmation: source.confirmation,
            work_item: source.work_item,
            receipt,
        },
        command.event_id.to_string(),
    ))
}

/// 回放只接受确认事实和正式任务共同证明的原提交结果。
async fn replay_confirmation_step(
    db: &mongodb::Database,
    identity: &StructuredCommandReceipt,
    prepared: &PreparedConfirmationCompletion,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<CompleteImportBusinessConfirmationResult>> {
    let Some(fact) = db.import_command_receipts().find_by_id(&identity.command_id, executor).await? else {
        return Ok(None);
    };
    fact.ensure_identity(identity)?;
    let ImportCommandResult::Confirmation(outcome) = &fact.result else {
        return Err(Error::ConflictError("导入确认回执结果类型不一致".into()));
    };
    let confirmation = db
        .legacy_import_confirmations()
        .find_by_work_item(&prepared.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::Internal("导入确认回执对应事实缺失".into()))?;
    let work_item = db
        .work_items()
        .find_by_id(prepared.work_item_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::Internal("导入确认回执对应任务缺失".into()))?;
    validate_confirmation_replay(outcome, &confirmation, &work_item, prepared, actor.id())?;
    work_item_service(db.clone(), crate::adapters::identity::shared_rbac_service(db.clone()))
        .ensure_domain_decision_access(actor, &work_item, executor)
        .await?;
    Ok(Some(completion_result(
        ConfirmationCompletionTransactionResult { confirmation, work_item, receipt: outcome.receipt },
        fact.audit_event_id,
    )))
}

/// 原决定、责任、任务主体、版本和完成时间必须交叉证明，不能只凭回执恢复。
/// # 参数
/// * `outcome` - 回执冻结的原确认结果。
/// * `confirmation` - 当前正式确认事实。
/// * `task` - 当前正式任务终态。
/// * `prepared` - 完整规范化原命令。
/// * `actor_id` - 当前认证操作人。
/// # 返回
/// 所有原提交事实和任务引用一致时返回空值。
/// # 错误
/// 缺失或不匹配事实时拒绝恢复。
pub(super) fn validate_confirmation_replay(
    outcome: &ImportConfirmationOutcome,
    confirmation: &LegacyImportConfirmation,
    task: &WorkItem,
    prepared: &PreparedConfirmationCompletion,
    actor_id: &str,
) -> Result<()> {
    let scope = ConfirmationScope::parse(&prepared.confirmation_scope)?;
    if !confirmation_replay_fact_matches(outcome, confirmation, prepared, actor_id, scope.owner_role())
        || !confirmation_replay_task_matches(outcome, task, prepared, actor_id, scope.owner_role())
    {
        return Err(Error::ConflictError("导入确认回执与正式确认或任务终态不一致".into()));
    }
    Ok(())
}

/// 核对原确认决定和不可变回执，不依赖后续批次推进状态。
fn confirmation_replay_fact_matches(
    outcome: &ImportConfirmationOutcome,
    confirmation: &LegacyImportConfirmation,
    prepared: &PreparedConfirmationCompletion,
    actor_id: &str,
    owner_role: &str,
) -> bool {
    let expected_status = match prepared.decision {
        ConfirmationDecision::ConfirmScope => ConfirmationStatus::Confirmed,
        ConfirmationDecision::ReturnForFix => ConfirmationStatus::Rejected,
    };
    outcome.confirmation_id == confirmation.base.id
        && outcome.confirmation_version == confirmation.base.version
        && outcome.batch_id == prepared.batch_id.as_ref()
        && confirmation.batch_id == prepared.batch_id
        && !confirmation.base.is_deleted()
        && confirmation.trial_version == prepared.expected_trial_version
        && confirmation.owner_role == owner_role
        && LegacyImportConfirmation::subject_version(
            confirmation.batch_version,
            confirmation.trial_version,
            &confirmation.import_rule_version,
        ) == outcome.subject_version
        && confirmation.work_item_id == prepared.work_item_id
        && outcome.subject_version == prepared.expected_subject_version
        && outcome.confirmation_scope == prepared.confirmation_scope
        && confirmation.confirmation_scope == prepared.confirmation_scope
        && outcome.decision == prepared.decision
        && confirmation.decision == Some(prepared.decision)
        && confirmation.status == expected_status
        && confirmation.decided_by.as_deref() == Some(actor_id)
        && confirmation.decided_at == Some(outcome.decided_at)
        && confirmation.reason_code == prepared.reason_code
        && confirmation.comment == prepared.comment
        && Some(outcome.receipt.batch_version) == prepared.expected_batch_version.checked_add(1)
}

/// 完成后的任务仍须保留原责任人；开放任务资格不适用于终态重放。
fn confirmation_replay_task_matches(
    outcome: &ImportConfirmationOutcome,
    task: &WorkItem,
    prepared: &PreparedConfirmationCompletion,
    actor_id: &str,
    owner_role: &str,
) -> bool {
    !task.base.is_deleted()
        && outcome.work_item_id == task.base.id
        && task.base.id == prepared.work_item_id.as_ref()
        && task.subject_version == outcome.subject_version
        && task.status == WorkItemStatus::Completed
        && task.base.version == outcome.receipt.task_version
        && Some(outcome.receipt.task_version) == prepared.expected_task_version.checked_add(1)
        && task.completed_by.as_deref() == Some(actor_id)
        && task.completed_at == Some(outcome.decided_at)
        && task.work_item_type == WorkItemType::ImportBusinessConfirmation
        && task.business_object_type == IMPORT_CONFIRMATION_OBJECT_TYPE
        && task.business_object_id == outcome.batch_id
        && task.responsibility_key() == Some(prepared.confirmation_scope.as_str())
        && task.owner_role == owner_role
        && task.owner_organization_id == IMPORT_CONFIRMATION_ORGANIZATION
        && task.owner_user_id.as_deref() == Some(actor_id)
}

struct ConfirmationCompletionTransactionResult {
    confirmation: LegacyImportConfirmation,
    work_item: WorkItem,
    receipt: ConfirmationCompletionReceipt,
}

/// 校验完成命令锁定的任务、事实、批次和当前责任。
pub(super) fn validate_confirmation_completion(
    command: &PreparedConfirmationCompletion,
    work_item: &WorkItem,
    confirmation: &LegacyImportConfirmation,
    batch: &LegacyImportBatch,
    actor_id: &str,
) -> Result<()> {
    if work_item.base.version != command.expected_task_version
        || !batch.has_version(command.expected_batch_version)
    {
        return Err(Error::ConflictError("导入确认任务或批次版本已变化，请刷新后重试".to_string()));
    }
    let expected_subject = LegacyImportConfirmation::subject_version(
        confirmation.batch_version,
        confirmation.trial_version,
        &confirmation.import_rule_version,
    );
    if command.expected_subject_version != expected_subject
        || work_item.subject_version != expected_subject
        || confirmation.trial_version != command.expected_trial_version
    {
        return Err(Error::ConflictError("导入确认的批次或试算快照已变化".to_string()));
    }
    let owner_role = ConfirmationScope::parse(&command.confirmation_scope)?.owner_role();
    let task_matches = work_item.work_item_type == WorkItemType::ImportBusinessConfirmation
        && work_item.business_object_type == IMPORT_CONFIRMATION_OBJECT_TYPE
        && work_item.business_object_id == confirmation.batch_id.to_string()
        && work_item.responsibility_key() == Some(command.confirmation_scope.as_str())
        && work_item.owner_role == owner_role
        && work_item.owner_organization_id == IMPORT_CONFIRMATION_ORGANIZATION;
    let fact_matches = confirmation.work_item_id == command.work_item_id
        && confirmation.batch_id == command.batch_id
        && confirmation.confirmation_scope == command.confirmation_scope
        && confirmation.owner_role == owner_role
        && confirmation.is_pending()
        && batch.base.id == confirmation.batch_id.to_string()
        && batch.accepts_confirmation_decision()
        && batch.import_rule_version == confirmation.import_rule_version;
    if !task_matches || !fact_matches {
        return Err(Error::BusinessLogicError("导入确认任务、责任范围或批次不匹配".to_string()));
    }
    let command_scope = ConfirmationScope::parse(&command.confirmation_scope)?;
    if !batch.required_confirmation_scopes()?.contains(&command_scope) {
        return Err(Error::BusinessLogicError("当前确认范围已不属于批次必要矩阵".to_string()));
    }
    if !work_item.is_owned_by(actor_id) {
        return Err(Error::Forbidden("当前账号不是该导入确认的当前责任人".to_string()));
    }
    Ok(())
}

/// 把事务结果组装为强类型响应信封。
fn completion_result(
    result: ConfirmationCompletionTransactionResult,
    audit_receipt: String,
) -> CompleteImportBusinessConfirmationResult {
    CompleteImportBusinessConfirmationResult {
        result_status: result.receipt.result_status,
        confirmation: confirmation_view(result.confirmation, &result.work_item),
        work_item: work_item_view(&result.work_item),
        batch_version: result.receipt.batch_version,
        next_step: result.receipt.next_step,
        audit_receipt,
    }
}

/// 返回决策对应的稳定结果状态。
pub(super) fn confirmation_result_status(
    decision: ConfirmationDecision,
) -> ImportBusinessConfirmationResultStatus {
    match decision {
        ConfirmationDecision::ConfirmScope => ImportBusinessConfirmationResultStatus::Confirmed,
        ConfirmationDecision::ReturnForFix => ImportBusinessConfirmationResultStatus::Rejected,
    }
}

/// 构造导入确认命令的领域幂等身份。
///
/// # 参数
/// * `actor_id` - 当前确认人
/// * `action` - 稳定命令动作
/// * `command` - 已解析并规范化的确认命令
///
/// # 返回
/// 返回不暴露原始幂等键的命令 ID 与完整命令指纹。
pub(super) fn confirmation_command_identity(
    actor_id: &str,
    action: &str,
    command: &PreparedConfirmationCompletion,
) -> LegacyImportCommandIdentity {
    let task_version = command.expected_task_version.to_string();
    let batch_version = command.expected_batch_version.to_string();
    let trial_version = command.expected_trial_version.to_string();
    LegacyImportCommandIdentity::new(
        IMPORT_CONFIRMATION_COMMAND_PREFIX,
        actor_id,
        action,
        command.work_item_id.as_ref(),
        &command.idempotency_key,
        &[
            command.work_item_id.as_ref(),
            command.batch_id.as_ref(),
            &task_version,
            &command.expected_subject_version,
            &batch_version,
            &trial_version,
            &command.confirmation_scope,
            command.decision.as_str(),
            command.reason_code.as_deref().unwrap_or_default(),
            command.comment.as_deref().unwrap_or_default(),
        ],
    )
}
