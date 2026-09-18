use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_import::repository::prelude::*;
use erp_import::{
    CompleteImportBusinessConfirmationCommand, ConfirmationDecision, ConfirmationScope,
    ImportBusinessConfirmationNextStep, ImportBusinessConfirmationResultStatus, LegacyImportBatch,
    LegacyImportBatchStatus, LegacyImportCommandIdentity, LegacyImportConfirmation, LegacyImportExt,
    PreparedConfirmationCompletion, parse_receipt_number,
};
use erp_workflow::entity::document_registry::WorkflowActionId;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::{DocumentRegistryExt, WorkItemExt};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::confirmation_query::{confirmation_view, work_item_view};
use super::create_confirmation::{confirmation_next_step, replace_confirmation_in_matrix};
use super::dto::CompleteImportBusinessConfirmationResult;
use super::{
    COMMAND_FINGERPRINT_PREFIX, IMPORT_CONFIRMATION_AUDIT_PREFIX, IMPORT_CONFIRMATION_OBJECT_TYPE,
    IMPORT_CONFIRMATION_ORGANIZATION, ImportApplyService,
};
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

impl ImportApplyService {
    /// 执行 `CompleteImportBusinessConfirmation` 强类型命令。
    ///
    /// 确认事实、批次摘要/阶段、`workflow_action`、任务完成与稳定审计
    /// 收据在同一事务提交。同一幂等键只有在全部命令字段一致时才返回原结果。
    ///
    /// # 参数
    /// * `req` - 强类型完成命令
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认事实、已完成任务、批次新版本、下一步和审计收据。
    ///
    /// # 错误
    /// * `NotFound` - 任务、确认事实或批次不存在
    /// * `ConflictError` - 任务/批次/试算版本或幂等指纹不一致
    /// * `Forbidden` - 当前用户不是任务责任人或已失去责任资格
    pub async fn complete_import_business_confirmation(
        &self,
        req: CompleteImportBusinessConfirmationCommand,
        actor: &AuditActor,
    ) -> Result<CompleteImportBusinessConfirmationResult> {
        req.validate()?;
        let prepared = PreparedConfirmationCompletion::try_from(req)?;
        let action = "legacy_import_confirmation.complete";
        let identity = confirmation_command_identity(actor.id(), action, &prepared);
        let fingerprint = identity.fingerprint().to_string();
        let audit_id = identity.audit_id().to_string();
        if let Some(result) = self.replay_confirmation_completion(&audit_id, &fingerprint, &prepared).await? {
            return Ok(result);
        }
        let decided_at = Instant::now();
        let workflow_action_id = WorkflowActionId::new(next_id());
        let db = self.db.clone();
        let client = db.client().clone();
        let prepared_for_tx = prepared.clone();
        let actor_id = actor.id().to_string();
        let audit_actor = actor.clone();
        let rbac_for_tx = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut work_item = db
                        .work_items()
                        .find_by_id(prepared_for_tx.work_item_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入确认任务不存在".to_string()))?;
                    let mut confirmation = db
                        .legacy_import_confirmations()
                        .find_by_work_item(&prepared_for_tx.work_item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入确认事实不存在".to_string()))?;
                    let mut batch = db
                        .legacy_import_batches()
                        .find_by_id(confirmation.batch_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
                    validate_confirmation_completion(
                        &prepared_for_tx,
                        &work_item,
                        &confirmation,
                        &batch,
                        &actor_id,
                    )?;
                    work_item_service(db.clone(), rbac_for_tx.clone())
                        .ensure_domain_decision_access(&audit_actor, &work_item, session)
                        .await?;
                    let _ = &work_item;
                    let mut matrix = db
                        .legacy_import_confirmations()
                        .list_by_batch(&confirmation.batch_id, session)
                        .await?;
                    confirmation.decide(
                        prepared_for_tx.decision,
                        actor_id.clone(),
                        decided_at,
                        prepared_for_tx.reason_code.clone(),
                        prepared_for_tx.comment.clone(),
                    )?;
                    work_item.record_activity(&actor_id, decided_at)?;
                    work_item.complete_by_domain_command(actor_id.clone(), decided_at)?;
                    replace_confirmation_in_matrix(&mut matrix, &confirmation);
                    let current_matrix = LegacyImportConfirmation::current_matrix(
                        &matrix,
                        confirmation.batch_version,
                        confirmation.trial_version,
                        &confirmation.import_rule_version,
                    );
                    let required_scopes = batch.required_confirmation_scopes()?;
                    let next_step = confirmation_next_step(LegacyImportConfirmation::matrix_decision(
                        prepared_for_tx.decision,
                        &current_matrix,
                        &required_scopes,
                    ));
                    batch.update_summaries(
                        batch.failure_code_summary.clone(),
                        Some(LegacyImportConfirmation::matrix_summary(
                            confirmation.trial_version,
                            &current_matrix,
                        )),
                    )?;
                    if next_step == ImportBusinessConfirmationNextStep::StartApply {
                        batch.advance(LegacyImportBatchStatus::ReadyToApply)?;
                    }
                    let workflow_action = super::factories::confirmation_workflow_action(
                        workflow_action_id,
                        &confirmation,
                        &actor_id,
                    )?;
                    db.legacy_import_confirmations().update(&mut confirmation, session).await?;
                    db.legacy_import_batches().update(&mut batch, session).await?;
                    db.work_items().update(&mut work_item, session).await?;
                    db.workflow_actions().create(&workflow_action, session).await?;
                    let receipt = ConfirmationCompletionReceipt {
                        result_status: confirmation_result_status(prepared_for_tx.decision),
                        task_version: work_item.base.version,
                        batch_version: batch.base.version,
                        next_step,
                    };
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx.clone(),
                        action,
                        "legacy_import_confirmation",
                        confirmation.base.id.clone(),
                        Some(confirmation_completion_receipt_message(&fingerprint_for_tx, receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ConfirmationCompletionTransactionResult, crate::Error>(
                        ConfirmationCompletionTransactionResult { confirmation, work_item, receipt },
                    )
                })
            })
            .await;
        let result = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                match self.replay_confirmation_completion(&audit_id, &fingerprint, &prepared).await? {
                    Some(result) => return Ok(result),
                    None => return Err(error),
                }
            },
        };

        Ok(completion_result(result, audit_id))
    }

    /// 按稳定审计收据重放已提交的导入确认命令。
    async fn replay_confirmation_completion(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        prepared: &PreparedConfirmationCompletion,
    ) -> Result<Option<CompleteImportBusinessConfirmationResult>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        let receipt = parse_confirmation_completion_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("导入确认幂等收据缺少结果".to_string()))?,
            expected_fingerprint,
        )?;
        let confirmation = self
            .db
            .legacy_import_confirmations()
            .find_by_work_item(&prepared.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入确认幂等收据对应事实缺失".to_string()))?;
        if audit.resource_id.as_deref() != Some(&confirmation.base.id) {
            return Err(Error::Internal("导入确认幂等收据与业务事实不一致".to_string()));
        }
        let work_item = self
            .db
            .work_items()
            .find_by_id(prepared.work_item_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入确认幂等收据对应任务缺失".to_string()))?;
        if work_item.status != WorkItemStatus::Completed
            || work_item.base.version != receipt.task_version
            || confirmation.decision != Some(prepared.decision)
        {
            return Err(Error::Internal("导入确认幂等收据对应结果不一致".to_string()));
        }
        Ok(Some(completion_result(
            ConfirmationCompletionTransactionResult { confirmation, work_item, receipt },
            audit_id.to_string(),
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConfirmationCompletionReceipt {
    pub(super) result_status: ImportBusinessConfirmationResultStatus,
    pub(super) task_version: u64,
    pub(super) batch_version: u64,
    pub(super) next_step: ImportBusinessConfirmationNextStep,
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
/// * `action` - 稳定审计动作
/// * `command` - 已解析并规范化的确认命令
///
/// # 返回
/// 返回不暴露原始幂等键的审计 ID 与完整命令指纹。
pub(super) fn confirmation_command_identity(
    actor_id: &str,
    action: &str,
    command: &PreparedConfirmationCompletion,
) -> LegacyImportCommandIdentity {
    let task_version = command.expected_task_version.to_string();
    let batch_version = command.expected_batch_version.to_string();
    let trial_version = command.expected_trial_version.to_string();
    LegacyImportCommandIdentity::new(
        IMPORT_CONFIRMATION_AUDIT_PREFIX,
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

/// 将导入确认的最小结果收据编码到审计消息。
pub(super) fn confirmation_completion_receipt_message(
    fingerprint: &str,
    receipt: ConfirmationCompletionReceipt,
) -> String {
    let result = match receipt.result_status {
        ImportBusinessConfirmationResultStatus::Confirmed => "C",
        ImportBusinessConfirmationResultStatus::Rejected => "R",
        ImportBusinessConfirmationResultStatus::Unknown => "U",
    };
    let next = match receipt.next_step {
        ImportBusinessConfirmationNextStep::AwaitOtherConfirmations => "W",
        ImportBusinessConfirmationNextStep::StartApply => "A",
        ImportBusinessConfirmationNextStep::FixAndRevalidate => "F",
    };
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={result}|{}|{}|{next}",
        receipt.task_version, receipt.batch_version
    )
}

/// 解析并核对导入确认审计收据。
pub(super) fn parse_confirmation_completion_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<ConfirmationCompletionReceipt> {
    let (fingerprint, encoded) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("导入确认幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError("幂等键已用于不同的导入确认命令".to_string()));
    }
    let fields = encoded.split('|').collect::<Vec<_>>();
    let [result, task_version, batch_version, next] = fields.as_slice() else {
        return Err(Error::Internal("导入确认幂等收据结果非法".to_string()));
    };
    let result_status = match *result {
        "C" => ImportBusinessConfirmationResultStatus::Confirmed,
        "R" => ImportBusinessConfirmationResultStatus::Rejected,
        "U" => ImportBusinessConfirmationResultStatus::Unknown,
        _ => return Err(Error::Internal("导入确认幂等收据状态非法".to_string())),
    };
    let next_step = match *next {
        "W" => ImportBusinessConfirmationNextStep::AwaitOtherConfirmations,
        "A" => ImportBusinessConfirmationNextStep::StartApply,
        "F" => ImportBusinessConfirmationNextStep::FixAndRevalidate,
        _ => return Err(Error::Internal("导入确认幂等收据下一步非法".to_string())),
    };
    Ok(ConfirmationCompletionReceipt {
        result_status,
        task_version: parse_receipt_number(task_version, "任务版本")?,
        batch_version: parse_receipt_number(batch_version, "批次版本")?,
        next_step,
    })
}
