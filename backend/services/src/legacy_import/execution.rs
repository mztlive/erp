use database::{AccessControlExt, BulkJobExt, Executor, LegacyImportExt, NoTransaction, Transactional};
use entities::bulk_job::{BackgroundJob, JobStatus};
use entities::common::time::Instant;
use entities::legacy_import::{
    ImportStatus, LegacyImportBatch, LegacyImportBatchStatus, LegacyImportCommandIdentity,
    LegacyImportConfirmation, LegacyImportRow,
};
use entities::AuditLog;
use mongodb::Database;
use std::collections::BTreeSet;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    ImportExecutionAction, ImportExecutionCommand, ImportExecutionNextStep, ImportExecutionResult,
    ImportExecutionResultStatus, PreparedImportExecution,
};
use super::receipt::parse_receipt_number;
use super::{LegacyImportService, COMMAND_FINGERPRINT_PREFIX, IMPORT_EXECUTION_AUDIT_PREFIX};

impl LegacyImportService {
    /// 执行 W18 导入应用阶段强命令。
    ///
    /// `START_APPLY` 是唯一能将批次推进到 `Importing` 并启动后台
    /// 任务的命令；`CANCEL_PENDING` 只停止未应用项；`RETRY_FAILED`
    /// 只重新准备失败行，仍需后续显式提交应用。批次、行、后台任务、
    /// 审计与幂等收据在同一事务提交。
    ///
    /// # 参数
    /// * `batch_id` - HTTP 路径中的导入批次 ID
    /// * `command` - 带批次/试算版本和请求幂等身份的强命令
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已提交的批次/后台任务状态、影响项数、下一步与审计收据。
    ///
    /// # 错误
    /// * `ValidationError` - 命令形状、版本或动作必填原因非法
    /// * `ConflictError` - 批次/试算版本过期，或同一请求身份被不同载荷复用
    /// * `BusinessLogicError` - 批次、行或后台任务状态不允许动作
    pub async fn execute_import_command(
        &self,
        batch_id: &str,
        command: ImportExecutionCommand,
        actor: &AuditActor,
    ) -> Result<ImportExecutionResult> {
        command.validate()?;
        let prepared = PreparedImportExecution::try_from(command)?;
        if prepared.batch_id.as_ref() != batch_id {
            return Err(Error::ValidationError("路径批次与执行命令批次不一致".to_string()));
        }
        let action_name = "legacy_import_batch.execute";
        let identity = import_execution_command_identity(actor.id(), action_name, &prepared);
        let fingerprint = identity.fingerprint().to_string();
        let audit_id = identity.audit_id().to_string();
        if let Some(result) = self
            .replay_import_execution(&audit_id, &fingerprint, &prepared)
            .await?
        {
            return Ok(result);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let prepared_for_tx = prepared.clone();
        let audit_actor = actor.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    execute_import_command_transaction(
                        &db,
                        &prepared_for_tx,
                        &audit_actor,
                        &audit_id_for_tx,
                        &fingerprint_for_tx,
                        action_name,
                        session,
                    )
                    .await
                })
            })
            .await;
        let result = match transaction_result {
            Ok(result) => result,
            Err(error) => match self
                .replay_import_execution(&audit_id, &fingerprint, &prepared)
                .await?
            {
                Some(result) => return Ok(result),
                None => return Err(error),
            },
        };

        Ok(import_execution_result(result, audit_id))
    }

    /// 按稳定审计收据重放已提交的导入执行命令。
    async fn replay_import_execution(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        prepared: &PreparedImportExecution,
    ) -> Result<Option<ImportExecutionResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        let receipt = parse_import_execution_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("导入执行幂等收据缺少结果".to_string()))?,
            expected_fingerprint,
        )?;
        let batch = self
            .db
            .legacy_import_batches()
            .find_by_id(prepared.batch_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入执行收据对应批次缺失".to_string()))?;
        let job = self
            .db
            .background_jobs()
            .find_by_request_id(&batch.batch_no, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入执行收据对应后台任务缺失".to_string()))?;
        validate_import_execution_replay(&audit, &batch, &job, &receipt, prepared.action)?;
        Ok(Some(import_execution_result(
            ImportExecutionTransactionResult { batch, job, receipt },
            audit_id.to_string(),
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImportExecutionReceipt {
    action: ImportExecutionAction,
    result_status: ImportExecutionResultStatus,
    batch_version: u64,
    batch_status: LegacyImportBatchStatus,
    trial_version: Option<u32>,
    job_version: u64,
    job_status: JobStatus,
    affected_items: u64,
    next_step: ImportExecutionNextStep,
}

struct ImportExecutionTransactionResult {
    batch: LegacyImportBatch,
    job: BackgroundJob,
    receipt: ImportExecutionReceipt,
}

struct ImportExecutionActionOutcome {
    result_status: ImportExecutionResultStatus,
    next_step: ImportExecutionNextStep,
    affected_items: u64,
}

/// 在一个持久化事务内执行导入应用强命令。
async fn execute_import_command_transaction(
    db: &Database,
    prepared: &PreparedImportExecution,
    audit_actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    action_name: &str,
    executor: &mut dyn Executor,
) -> Result<ImportExecutionTransactionResult> {
    let mut batch = db
        .legacy_import_batches()
        .find_by_id(prepared.batch_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
    if !batch.has_version(prepared.expected_batch_version) {
        return Err(Error::ConflictError(
            "导入批次版本已变化，请刷新后重试".to_string(),
        ));
    }
    let mut job = db
        .background_jobs()
        .find_by_request_id(&batch.batch_no, executor)
        .await?
        .ok_or_else(|| Error::Internal("导入批次后台任务缺失".to_string()))?;
    validate_import_background_job(&batch, &job)?;
    let confirmations = db
        .legacy_import_confirmations()
        .list_by_batch(&prepared.batch_id, executor)
        .await?;
    let trial_version = validate_import_execution_trial(prepared, &batch, &confirmations)?;
    let mut rows = if prepared.action == ImportExecutionAction::RetryFailed {
        db.legacy_import_rows()
            .list_failed_by_batch(&prepared.batch_id, executor)
            .await?
    } else {
        Vec::new()
    };
    let outcome = apply_import_execution_action(prepared, &mut batch, &mut job, &mut rows)?;
    if prepared.action == ImportExecutionAction::RetryFailed {
        db.legacy_import_rows()
            .persist_failed_retry_rows(&mut rows, executor)
            .await?;
    }
    db.legacy_import_batches().update(&mut batch, executor).await?;
    db.background_jobs().update(&mut job, executor).await?;
    let receipt = ImportExecutionReceipt {
        action: prepared.action,
        result_status: outcome.result_status,
        batch_version: batch.base.version,
        batch_status: batch.status,
        trial_version,
        job_version: job.base.version,
        job_status: job.status,
        affected_items: outcome.affected_items,
        next_step: outcome.next_step,
    };
    let audit = audit_actor.clone().resource_log_with_id(
        audit_id.to_string(),
        action_name,
        "legacy_import_batch",
        batch.base.id.clone(),
        Some(import_execution_receipt_message(fingerprint, receipt)),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(ImportExecutionTransactionResult { batch, job, receipt })
}

/// 核对批次与后台任务的稳定关联和计数。
fn validate_import_background_job(batch: &LegacyImportBatch, job: &BackgroundJob) -> Result<()> {
    let matches = job.job_type == entities::bulk_job::JobType::Import
        && job.domain_job_type.as_deref() == Some(entities::bulk_job::LEGACY_IMPORT_DOMAIN_JOB_TYPE)
        && job.domain_job_id.as_deref() == Some(batch.base.id.as_str())
        && job.request_id == batch.batch_no
        && job.total_count == batch.total_rows;
    if matches {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "导入批次与后台任务关联不一致".to_string(),
    ))
}

/// 核对执行命令锁定的当前试算和全部必要确认。
fn validate_import_execution_trial(
    prepared: &PreparedImportExecution,
    batch: &LegacyImportBatch,
    confirmations: &[LegacyImportConfirmation],
) -> Result<Option<u32>> {
    let Some(expected_trial) = prepared.expected_trial_version else {
        return Ok(None);
    };
    let current_trial =
        LegacyImportConfirmation::latest_active_trial(confirmations, &batch.import_rule_version)
            .ok_or_else(|| Error::BusinessLogicError("当前批次缺少有效试算确认".to_string()))?;
    if current_trial != expected_trial {
        return Err(Error::ConflictError(
            "导入试算版本已变化，请刷新后重试".to_string(),
        ));
    }
    if matches!(
        prepared.action,
        ImportExecutionAction::StartApply | ImportExecutionAction::RetryFailed
    ) {
        ensure_import_trial_confirmed(batch, confirmations, current_trial)?;
    }
    Ok(Some(current_trial))
}

/// 确保当前试算的全部必要责任范围均已确认。
fn ensure_import_trial_confirmed(
    batch: &LegacyImportBatch,
    confirmations: &[LegacyImportConfirmation],
    trial_version: u32,
) -> Result<()> {
    let required = batch.required_confirmation_scopes()?;
    if !LegacyImportConfirmation::is_trial_confirmed(
        confirmations,
        trial_version,
        &batch.import_rule_version,
        &required,
    ) {
        return Err(Error::BusinessLogicError(
            "当前试算尚未完成全部必要责任确认".to_string(),
        ));
    }
    Ok(())
}

/// 仅在内存中应用执行动作，持久化由外层事务统一完成。
fn apply_import_execution_action(
    prepared: &PreparedImportExecution,
    batch: &mut LegacyImportBatch,
    job: &mut BackgroundJob,
    rows: &mut [LegacyImportRow],
) -> Result<ImportExecutionActionOutcome> {
    match prepared.action {
        ImportExecutionAction::StartApply => start_import_application(batch, job),
        ImportExecutionAction::CancelPending => cancel_pending_import(batch, job),
        ImportExecutionAction::RetryFailed => prepare_failed_import_retry(batch, job, rows),
    }
}

/// 显式启动导入应用和关联后台任务。
fn start_import_application(
    batch: &mut LegacyImportBatch,
    job: &mut BackgroundJob,
) -> Result<ImportExecutionActionOutcome> {
    if !batch.is_ready_to_apply() || job.status != JobStatus::Pending {
        return Err(Error::BusinessLogicError(
            "只有待应用批次与等待执行任务可以提交应用".to_string(),
        ));
    }
    let affected_items = job
        .total_count
        .checked_sub(job.processed_count)
        .ok_or_else(|| Error::Internal("后台任务进度计数异常".to_string()))?;
    if affected_items == 0 {
        return Err(Error::BusinessLogicError("当前批次没有待应用项".to_string()));
    }
    batch.advance(LegacyImportBatchStatus::Importing)?;
    job.start(Instant::now())?;
    Ok(ImportExecutionActionOutcome {
        result_status: ImportExecutionResultStatus::Started,
        next_step: ImportExecutionNextStep::MonitorProgress,
        affected_items,
    })
}

/// 取消尚未应用项，已处理计数和行事实原样保留。
fn cancel_pending_import(
    batch: &mut LegacyImportBatch,
    job: &mut BackgroundJob,
) -> Result<ImportExecutionActionOutcome> {
    if !batch.accepts_pending_cancellation()
        || !matches!(
            job.status,
            JobStatus::Pending | JobStatus::Running | JobStatus::PartiallySucceeded
        )
    {
        return Err(Error::BusinessLogicError(
            "当前批次或后台任务状态不允许取消未应用项".to_string(),
        ));
    }
    let affected_items = job
        .total_count
        .checked_sub(job.processed_count)
        .ok_or_else(|| Error::Internal("后台任务进度计数异常".to_string()))?;
    if affected_items == 0 {
        return Err(Error::BusinessLogicError("没有可取消的未应用项".to_string()));
    }
    job.cancel(Instant::now())?;
    let outcome = if job.processed_count > 0 {
        LegacyImportBatchStatus::PartialFailed
    } else {
        LegacyImportBatchStatus::Failed
    };
    batch.advance(outcome)?;
    Ok(ImportExecutionActionOutcome {
        result_status: ImportExecutionResultStatus::Cancelled,
        next_step: ImportExecutionNextStep::ReviewResult,
        affected_items,
    })
}

/// 仅重新准备失败行，保留已导入、已跳过与未处理行。
///
/// 调用方事务已按 `list_failed_by_batch` 只装载失败子集；成功计数沿用批次
/// 已提交值，失败计数清零。子集行数必须与批次失败计数一致，否则失败关闭，
/// 避免用子集重算把成功计数归零。并发重试依赖每行 `id + version` 乐观锁。
///
/// # 参数
/// * `batch` - 失败或部分失败批次
/// * `job` - 后台任务
/// * `rows` - 失败行子集（调用方已按批次过滤）
///
/// # 返回
/// 返回重试影响项数与重试行 ID。
///
/// # 错误
/// 批次状态不允许、子集为空、子集与批次失败计数不一致或状态迁移失败时返回错误。
fn prepare_failed_import_retry(
    batch: &mut LegacyImportBatch,
    job: &mut BackgroundJob,
    rows: &mut [LegacyImportRow],
) -> Result<ImportExecutionActionOutcome> {
    if !batch.accepts_failed_retry() {
        return Err(Error::BusinessLogicError(
            "只有失败或部分失败批次可重新准备失败项".to_string(),
        ));
    }
    let retry_row_ids = rows
        .iter()
        .filter(|row| row.import_status == ImportStatus::Failed)
        .map(|row| row.base.id.clone())
        .collect::<BTreeSet<_>>();
    if retry_row_ids.is_empty() {
        return Err(Error::BusinessLogicError(
            "当前批次没有可重试的失败项".to_string(),
        ));
    }
    if retry_row_ids.len() as u64 != batch.failed_rows {
        return Err(Error::Internal("批次失败计数与失败行快照不一致".to_string()));
    }
    for row in rows.iter_mut().filter(|row| retry_row_ids.contains(&row.base.id)) {
        row.prepare_failed_retry()?;
    }
    let affected_items = retry_row_ids.len() as u64;
    job.prepare_failed_retry(affected_items, Instant::now())?;
    batch.update_counts(batch.total_rows, batch.success_rows, 0)?;
    batch.advance(LegacyImportBatchStatus::ReadyToApply)?;
    Ok(ImportExecutionActionOutcome {
        result_status: ImportExecutionResultStatus::RetryPrepared,
        next_step: ImportExecutionNextStep::StartApply,
        affected_items,
    })
}

/// 组装导入执行命令的稳定结果信封。
fn import_execution_result(
    result: ImportExecutionTransactionResult,
    audit_receipt: String,
) -> ImportExecutionResult {
    ImportExecutionResult {
        action: result.receipt.action,
        result_status: result.receipt.result_status,
        batch_id: result.batch.base.id,
        batch_status: result.receipt.batch_status,
        batch_version: result.receipt.batch_version.to_string(),
        trial_version: result.receipt.trial_version.map(|value| value.to_string()),
        background_job_id: result.job.base.id,
        background_job_status: result.receipt.job_status,
        background_job_version: result.receipt.job_version.to_string(),
        affected_items: result.receipt.affected_items,
        next_step: result.receipt.next_step,
        audit_receipt,
    }
}

/// 构造导入执行命令的领域幂等身份。
///
/// # 参数
/// * `actor_id` - 当前操作人
/// * `action` - 稳定审计动作
/// * `command` - 已解析并规范化的执行命令
///
/// # 返回
/// 返回不暴露原始请求 ID 的审计 ID 与完整命令指纹。
fn import_execution_command_identity(
    actor_id: &str,
    action: &str,
    command: &PreparedImportExecution,
) -> LegacyImportCommandIdentity {
    let batch_version = command.expected_batch_version.to_string();
    let trial_version = command
        .expected_trial_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    LegacyImportCommandIdentity::new(
        IMPORT_EXECUTION_AUDIT_PREFIX,
        actor_id,
        action,
        command.batch_id.as_ref(),
        &command.request_id,
        &[
            command.batch_id.as_ref(),
            &batch_version,
            &trial_version,
            command.action.as_str(),
            command.reason_code.as_deref().unwrap_or_default(),
            command.comment.as_deref().unwrap_or_default(),
        ],
    )
}

/// 将导入执行最小结果收据编码到审计消息。
fn import_execution_receipt_message(fingerprint: &str, receipt: ImportExecutionReceipt) -> String {
    let result = match receipt.result_status {
        ImportExecutionResultStatus::Started => "S",
        ImportExecutionResultStatus::Cancelled => "C",
        ImportExecutionResultStatus::RetryPrepared => "R",
        ImportExecutionResultStatus::Unknown => "U",
    };
    let next = match receipt.next_step {
        ImportExecutionNextStep::MonitorProgress => "M",
        ImportExecutionNextStep::ReviewResult => "V",
        ImportExecutionNextStep::StartApply => "A",
    };
    let trial = receipt
        .trial_version
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};execution={}|{result}|{}|{}|{trial}|{}|{}|{}|{next}",
        receipt.action.as_str(),
        receipt.batch_version,
        receipt.batch_status.as_str(),
        receipt.job_version,
        receipt.job_status.as_str(),
        receipt.affected_items,
    )
}

/// 解析并核对导入执行审计收据。
fn parse_import_execution_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<ImportExecutionReceipt> {
    let (fingerprint, encoded) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";execution="))
        .ok_or_else(|| Error::Internal("导入执行幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(
            "请求身份已用于不同的导入执行命令".to_string(),
        ));
    }
    let fields = encoded.split('|').collect::<Vec<_>>();
    let [action, result, batch_version, batch_status, trial, job_version, job_status, affected, next] =
        fields.as_slice()
    else {
        return Err(Error::Internal("导入执行幂等收据结果非法".to_string()));
    };
    Ok(ImportExecutionReceipt {
        action: parse_import_execution_action(action)?,
        result_status: parse_import_execution_result_status(result)?,
        batch_version: parse_receipt_number(batch_version, "批次版本")?,
        batch_status: parse_import_batch_status(batch_status)?,
        trial_version: if *trial == "-" {
            None
        } else {
            Some(parse_receipt_number(trial, "试算版本")?)
        },
        job_version: parse_receipt_number(job_version, "后台任务版本")?,
        job_status: parse_background_job_status(job_status)?,
        affected_items: parse_receipt_number(affected, "影响项数")?,
        next_step: parse_import_execution_next_step(next)?,
    })
}

/// 核对幂等收据与当前批次/后台任务的稳定身份关联。
///
/// 当前版本与状态可能已被后台进度继续推进，重放必须返回收据记录的
/// 原始结果，不能要求当前快照仍停留在命令提交时刻。
fn validate_import_execution_replay(
    audit: &AuditLog,
    batch: &LegacyImportBatch,
    job: &BackgroundJob,
    receipt: &ImportExecutionReceipt,
    expected_action: ImportExecutionAction,
) -> Result<()> {
    let exact = audit.resource_id.as_deref() == Some(batch.base.id.as_str())
        && receipt.action == expected_action
        && job.domain_job_id.as_deref() == Some(batch.base.id.as_str())
        && job.request_id == batch.batch_no;
    if exact {
        return Ok(());
    }
    Err(Error::Internal(
        "导入执行幂等收据与当前业务事实不一致".to_string(),
    ))
}

/// 解析导入执行动作 wire code。
fn parse_import_execution_action(value: &str) -> Result<ImportExecutionAction> {
    match value {
        "START_APPLY" => Ok(ImportExecutionAction::StartApply),
        "CANCEL_PENDING" => Ok(ImportExecutionAction::CancelPending),
        "RETRY_FAILED" => Ok(ImportExecutionAction::RetryFailed),
        _ => Err(Error::Internal("导入执行收据动作非法".to_string())),
    }
}

/// 解析导入执行结果 wire code。
fn parse_import_execution_result_status(value: &str) -> Result<ImportExecutionResultStatus> {
    match value {
        "S" => Ok(ImportExecutionResultStatus::Started),
        "C" => Ok(ImportExecutionResultStatus::Cancelled),
        "R" => Ok(ImportExecutionResultStatus::RetryPrepared),
        "U" => Ok(ImportExecutionResultStatus::Unknown),
        _ => Err(Error::Internal("导入执行收据结果非法".to_string())),
    }
}

/// 解析导入执行下一步 wire code。
fn parse_import_execution_next_step(value: &str) -> Result<ImportExecutionNextStep> {
    match value {
        "M" => Ok(ImportExecutionNextStep::MonitorProgress),
        "V" => Ok(ImportExecutionNextStep::ReviewResult),
        "A" => Ok(ImportExecutionNextStep::StartApply),
        _ => Err(Error::Internal("导入执行收据下一步非法".to_string())),
    }
}

/// 解析导入批次状态稳定码。
fn parse_import_batch_status(value: &str) -> Result<LegacyImportBatchStatus> {
    match value {
        "pending_validation" => Ok(LegacyImportBatchStatus::PendingValidation),
        "validating" => Ok(LegacyImportBatchStatus::Validating),
        "pending_confirmation" => Ok(LegacyImportBatchStatus::PendingConfirmation),
        "ready_to_apply" => Ok(LegacyImportBatchStatus::ReadyToApply),
        "importing" => Ok(LegacyImportBatchStatus::Importing),
        "completed" => Ok(LegacyImportBatchStatus::Completed),
        "partial_failed" => Ok(LegacyImportBatchStatus::PartialFailed),
        "failed" => Ok(LegacyImportBatchStatus::Failed),
        _ => Err(Error::Internal("导入执行收据批次状态非法".to_string())),
    }
}

/// 解析后台任务状态稳定码。
fn parse_background_job_status(value: &str) -> Result<JobStatus> {
    match value {
        "pending" => Ok(JobStatus::Pending),
        "running" => Ok(JobStatus::Running),
        "partially_succeeded" => Ok(JobStatus::PartiallySucceeded),
        "succeeded" => Ok(JobStatus::Succeeded),
        "failed" => Ok(JobStatus::Failed),
        "cancelled" => Ok(JobStatus::Cancelled),
        _ => Err(Error::Internal("导入执行收据后台任务状态非法".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::BusinessDate;
    use entities::ids::{ExternalIdentityMapId, LegacyImportBatchId, LegacyImportRowId, SourceSystemId};
    use entities::legacy_import::{LegacyImportBatchData, LegacyImportRowData, ParseStatus};

    use super::*;

    /// 经领域工厂登记测试后台任务（Service 只注入 ID 与发起人）。
    ///
    /// # 参数
    /// * `batch` - 导入批次实体
    /// * `actor` - 发起人账号 ID
    ///
    /// # 返回
    /// 返回新建的后台任务实体。
    fn test_background_job(batch: &LegacyImportBatch, actor: &str) -> entities::bulk_job::BackgroundJob {
        entities::bulk_job::BackgroundJob::for_legacy_import(
            entities::ids::BackgroundJobId::new(format!("job-{}", batch.batch_no)),
            &batch.batch_no,
            &batch.base.id,
            batch.total_rows,
            actor,
        )
        .unwrap()
    }

    fn batch() -> LegacyImportBatch {
        let mut batch = LegacyImportBatch::new(
            LegacyImportBatchId::new("batch-1"),
            LegacyImportBatchData {
                batch_no: "IMP-1".to_string(),
                source_system_id: SourceSystemId::new("source-1"),
                source_object_set: "CUSTOMER,CARD_OPENING_AR".to_string(),
                baseline_date: BusinessDate::from_ymd(2026, 8, 14).unwrap(),
                import_rule_version: "rule-1".to_string(),
                source_file_hmac: None,
                status: LegacyImportBatchStatus::PendingConfirmation,
                total_rows: 1,
                success_rows: 0,
                failed_rows: 0,
                failure_code_summary: None,
                confirmation_status_summary: None,
            },
        )
        .unwrap();
        batch.base.version = 4;
        batch
    }

    fn execution_command(action: ImportExecutionAction) -> PreparedImportExecution {
        PreparedImportExecution {
            batch_id: LegacyImportBatchId::new("batch-1"),
            expected_batch_version: 4,
            expected_trial_version: Some(2),
            action,
            reason_code: (action == ImportExecutionAction::CancelPending)
                .then(|| "USER_CANCELLED".to_string()),
            comment: None,
            request_id: "execution-1".to_string(),
        }
    }

    fn applicable_row(id: &str) -> LegacyImportRow {
        let mut row = LegacyImportRow::new(
            LegacyImportRowId::new(id),
            LegacyImportRowData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                source_object_type: "CONTRACT".to_string(),
                source_row_key: id.to_string(),
                normalized_payload_reference: format!("payload:{id}"),
            },
        )
        .unwrap();
        row.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        row.mark_mapped(ExternalIdentityMapId::new(format!("mapping:{id}")))
            .unwrap();
        row
    }

    #[test]
    fn start_apply_is_the_only_action_that_starts_background_job() {
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::ReadyToApply;
        let mut job = test_background_job(&import_batch, "admin-1");

        let outcome = start_import_application(&mut import_batch, &mut job).unwrap();

        assert_eq!(import_batch.status, LegacyImportBatchStatus::Importing);
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(outcome.result_status, ImportExecutionResultStatus::Started);
        assert_eq!(outcome.affected_items, import_batch.total_rows);
    }

    #[test]
    fn cancel_pending_preserves_processed_results_and_stops_only_remaining_items() {
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::Importing;
        import_batch.total_rows = 3;
        import_batch.success_rows = 1;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_progress(1, 0, 0, Instant::from_unix_secs(1_700_000_100))
            .unwrap();

        let outcome = cancel_pending_import(&mut import_batch, &mut job).unwrap();

        assert_eq!(import_batch.status, LegacyImportBatchStatus::PartialFailed);
        assert_eq!(import_batch.success_rows, 1);
        assert_eq!(job.status, JobStatus::Cancelled);
        assert_eq!(job.success_count, 1);
        assert_eq!(job.processed_count, 1);
        assert_eq!(outcome.affected_items, 2);
    }

    #[test]
    fn retry_failed_preserves_imported_and_skipped_rows_and_returns_ready() {
        let mut imported = applicable_row("imported");
        imported.mark_imported("SO-1".to_string(), None).unwrap();
        let mut skipped = applicable_row("skipped");
        skipped.mark_skipped("DUPLICATE".to_string(), None).unwrap();
        let mut failed = applicable_row("failed");
        failed.mark_import_failed("TEMPORARY".to_string(), None).unwrap();
        let mut rows = vec![imported, skipped, failed];
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::PartialFailed;
        import_batch.total_rows = 3;
        import_batch.success_rows = 1;
        import_batch.failed_rows = 1;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_import_result_batch(1, 1, 1, true, Instant::from_unix_secs(1_700_000_100))
            .unwrap();

        let outcome = prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).unwrap();

        assert_eq!(import_batch.status, LegacyImportBatchStatus::ReadyToApply);
        assert_eq!(import_batch.success_rows, 1);
        assert_eq!(import_batch.failed_rows, 0);
        assert_eq!(rows[0].import_status, ImportStatus::Imported);
        assert_eq!(rows[1].import_status, ImportStatus::Skipped);
        assert_eq!(rows[2].import_status, ImportStatus::PendingImport);
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.processed_count, 2);
        assert_eq!(job.success_count, 1);
        assert_eq!(job.skipped_count, 1);
        assert_eq!(outcome.affected_items, 1);
    }

    #[test]
    fn retry_failed_with_no_failed_rows_fails_closed() {
        let mut imported = applicable_row("imported");
        imported.mark_imported("SO-1".to_string(), None).unwrap();
        let mut rows = vec![imported];
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::PartialFailed;
        import_batch.total_rows = 1;
        import_batch.success_rows = 1;
        import_batch.failed_rows = 0;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        assert!(prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).is_err());
        assert!(prepare_failed_import_retry(&mut import_batch, &mut job, &mut []).is_err());
    }

    #[test]
    fn retry_failed_leaves_pending_rows_untouched_and_counts_large_batch() {
        let pending = applicable_row("pending");
        let mut failed = applicable_row("failed");
        failed.mark_import_failed("TEMPORARY".to_string(), None).unwrap();
        let mut rows = vec![pending, failed];
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::Failed;
        import_batch.total_rows = 2;
        import_batch.success_rows = 0;
        import_batch.failed_rows = 1;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_import_result_batch(0, 0, 1, false, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        let outcome = prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).unwrap();
        assert_eq!(rows[0].import_status, ImportStatus::PendingImport);
        assert_eq!(rows[1].import_status, ImportStatus::PendingImport);
        assert_eq!(outcome.affected_items, 1);
    }

    #[test]
    fn retry_failed_counts_large_failed_batch_in_single_pass() {
        let mut rows = Vec::new();
        for index in 0..300 {
            let mut row = applicable_row(&format!("failed-{index}"));
            row.mark_import_failed("TEMPORARY".to_string(), None).unwrap();
            rows.push(row);
        }
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::Failed;
        import_batch.total_rows = 300;
        import_batch.success_rows = 0;
        import_batch.failed_rows = 300;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_import_result_batch(0, 0, 300, true, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        let outcome = prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).unwrap();
        assert_eq!(outcome.affected_items, 300);
        assert!(rows
            .iter()
            .all(|row| row.import_status == ImportStatus::PendingImport));
    }

    #[test]
    fn retry_failed_only_subset_preserves_committed_success_counts() {
        let mut failed = applicable_row("failed");
        failed.mark_import_failed("TEMPORARY".to_string(), None).unwrap();
        let mut rows = vec![failed];
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::PartialFailed;
        import_batch.total_rows = 3;
        import_batch.success_rows = 2;
        import_batch.failed_rows = 1;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_import_result_batch(2, 0, 1, true, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        let outcome = prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).unwrap();
        assert_eq!(outcome.affected_items, 1);
        assert_eq!(import_batch.success_rows, 2);
        assert_eq!(import_batch.failed_rows, 0);
        assert_eq!(import_batch.status, LegacyImportBatchStatus::ReadyToApply);
        assert_eq!(rows[0].import_status, ImportStatus::PendingImport);
    }

    #[test]
    fn retry_failed_subset_mismatch_fails_closed() {
        let mut failed = applicable_row("failed");
        failed.mark_import_failed("TEMPORARY".to_string(), None).unwrap();
        let mut rows = vec![failed];
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::PartialFailed;
        import_batch.total_rows = 3;
        import_batch.success_rows = 1;
        import_batch.failed_rows = 2;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_import_result_batch(1, 0, 0, false, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert!(prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).is_err());
        assert_eq!(import_batch.success_rows, 1);
        assert_eq!(import_batch.failed_rows, 2);
    }

    #[test]
    fn execution_receipt_is_stable_and_rejects_request_reuse() {
        let command = execution_command(ImportExecutionAction::StartApply);
        let identity = import_execution_command_identity("user-1", "execute", &command);
        let fingerprint = identity.fingerprint().to_string();
        let receipt = ImportExecutionReceipt {
            action: ImportExecutionAction::StartApply,
            result_status: ImportExecutionResultStatus::Started,
            batch_version: 5,
            batch_status: LegacyImportBatchStatus::Importing,
            trial_version: Some(2),
            job_version: 2,
            job_status: JobStatus::Running,
            affected_items: 1,
            next_step: ImportExecutionNextStep::MonitorProgress,
        };
        let message = import_execution_receipt_message(&fingerprint, receipt);

        assert_eq!(
            parse_import_execution_receipt(&message, &fingerprint).unwrap(),
            receipt
        );
        assert!(parse_import_execution_receipt(&message, &"0".repeat(64)).is_err());
        assert!(!identity.audit_id().contains("execution-1"));
    }

    #[test]
    fn execution_receipt_replay_allows_later_background_progress() {
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::Completed;
        import_batch.base.version = 99;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.base.version = 42;
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        let receipt = ImportExecutionReceipt {
            action: ImportExecutionAction::StartApply,
            result_status: ImportExecutionResultStatus::Started,
            batch_version: 5,
            batch_status: LegacyImportBatchStatus::Importing,
            trial_version: Some(2),
            job_version: 2,
            job_status: JobStatus::Running,
            affected_items: 1,
            next_step: ImportExecutionNextStep::MonitorProgress,
        };
        let audit = AuditActor::new(
            "admin-1".to_string(),
            "admin".to_string(),
            entities::AccountKind::Admin,
        )
        .resource_log_with_id(
            "audit-1".to_string(),
            "legacy_import_batch.execute",
            "legacy_import_batch",
            import_batch.base.id.clone(),
            Some("receipt".to_string()),
        )
        .unwrap();

        assert!(validate_import_execution_replay(
            &audit,
            &import_batch,
            &job,
            &receipt,
            ImportExecutionAction::StartApply,
        )
        .is_ok());
    }
}
