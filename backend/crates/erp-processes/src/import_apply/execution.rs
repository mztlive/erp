use std::collections::BTreeSet;

use application_core::{AuditActor, StructuredCommandReceipt};
use erp_audit::BusinessEventContext;
use erp_core::common::time::Instant;
use erp_import::entity::command_receipt::{
    ImportCommandReceipt, ImportCommandResult, ImportExecutionReceipt,
};
use erp_import::repository::ImportCommandReceiptExt;
use erp_import::repository::prelude::*;
use erp_import::{
    ImportExecutionAction, ImportExecutionCommand, ImportExecutionNextStep, ImportExecutionResult,
    ImportExecutionResultStatus, ImportStatus, LegacyImportBatch, LegacyImportBatchStatus,
    LegacyImportCommandIdentity, LegacyImportConfirmation, LegacyImportExt, LegacyImportRow,
    PreparedImportExecution,
};
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::command_event::{import_event_content, import_event_context};
use super::{IMPORT_EXECUTION_COMMAND_PREFIX, ImportApplyService};
use crate::audit::{AuditedCommand, AuditedWrite, MongoAuditEventSink, execute_audited};
use crate::{Error, Result};

impl ImportApplyService {
    /// 执行 W18 应用阶段命令，行、批次、后台任务、独立回执及事件原子提交。
    /// # 参数
    /// * `batch_id` - 路径导入批次身份。
    /// * `command` - 完整版本化执行命令。
    /// * `actor` - 当前认证操作人。
    /// # 返回
    /// 返回命令原提交结果；后台进度可以继续推进。
    /// # 错误
    /// 命令、版本、状态或同键载荷冲突时拒绝执行。
    pub async fn execute_import_command(
        &self,
        batch_id: &str,
        command: ImportExecutionCommand,
        actor: &AuditActor,
    ) -> Result<ImportExecutionResult> {
        command.validate()?;
        let prepared = PreparedImportExecution::try_from(command)?;
        if prepared.batch_id.as_ref() != batch_id {
            return Err(Error::ValidationError("路径批次与执行命令批次不一致".into()));
        }
        let identity =
            import_execution_command_identity(actor.id(), "legacy_import_batch.execute", &prepared)
                .structured_receipt("legacy_import_batch")?;
        if let Some(result) = self.replay_import_execution(&identity, &prepared).await? {
            return Ok(result);
        }
        let context = import_event_context(
            actor,
            "legacy_import_batch.execute",
            "legacy_import_batch",
            "执行导入应用命令",
            &identity,
        )?;
        match self.commit_import_execution(&identity, &prepared, &context).await {
            Ok(result) => Ok(result),
            Err(error) => {
                crate::audit::recover_command(error, self.replay_import_execution(&identity, &prepared).await)
            },
        }
    }

    /// 同一只读事务核对领域回执、批次和后台任务身份。
    async fn replay_import_execution(
        &self,
        identity: &StructuredCommandReceipt,
        prepared: &PreparedImportExecution,
    ) -> Result<Option<ImportExecutionResult>> {
        let db = self.db.clone();
        let identity = identity.clone();
        let prepared = prepared.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { replay_execution_step(&db, &identity, &prepared, executor).await })
            })
            .await
    }

    /// 在原事务内执行类型化边界，不启动外部任务或改变后台生命周期。
    async fn commit_import_execution(
        &self,
        identity: &StructuredCommandReceipt,
        prepared: &PreparedImportExecution,
        context: &BusinessEventContext,
    ) -> Result<ImportExecutionResult> {
        let db = self.db.clone();
        let identity = identity.clone();
        let prepared = prepared.clone();
        let context = context.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let command = ImportExecutionWrite {
                        db: &db,
                        identity: &identity,
                        prepared: &prepared,
                        event_id: context.event_id(),
                    };
                    execute_audited(&context, &MongoAuditEventSink::new(&db), executor, &command).await
                })
            })
            .await
    }
}

/// 当前应用动作的实际业务执行边界。
struct ImportExecutionWrite<'a> {
    db: &'a Database,
    identity: &'a StructuredCommandReceipt,
    prepared: &'a PreparedImportExecution,
    event_id: &'a str,
}

#[async_trait::async_trait]
impl AuditedCommand for ImportExecutionWrite<'_> {
    type Output = ImportExecutionResult;

    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        if let Some(result) = replay_execution_step(self.db, self.identity, self.prepared, executor).await? {
            return Ok(AuditedWrite::Replayed(result));
        }
        let result = execute_import_command_transaction(self.db, self.prepared, executor).await?;
        let fact = ImportCommandReceipt::new(
            self.identity.clone(),
            ImportCommandResult::Execution {
                batch_id: result.batch.base.id.clone(),
                background_job_id: result.job.base.id.clone(),
                receipt: result.receipt,
            },
            self.event_id.to_string(),
        )?;
        self.db.import_command_receipts().create(&fact, executor).await?;
        Ok(AuditedWrite::Fresh {
            content: import_event_content(result.batch.base.id.clone()),
            result: import_execution_result(result, self.event_id.to_string()),
        })
    }
}

/// 只读恢复命令原结果；版本进展不能被误判为收据损坏。
async fn replay_execution_step(
    db: &Database,
    identity: &StructuredCommandReceipt,
    prepared: &PreparedImportExecution,
    executor: &mut dyn Executor,
) -> Result<Option<ImportExecutionResult>> {
    let Some(fact) = db.import_command_receipts().find_by_id(&identity.command_id, executor).await? else {
        return Ok(None);
    };
    fact.ensure_identity(identity)?;
    let ImportCommandResult::Execution { receipt, .. } = &fact.result else {
        return Err(Error::ConflictError("导入执行回执结果类型不一致".into()));
    };
    let batch = db
        .legacy_import_batches()
        .find_by_id(prepared.batch_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::Internal("导入执行回执对应批次缺失".into()))?;
    let job = db
        .background_jobs()
        .find_by_request_id(&batch.batch_no, executor)
        .await?
        .ok_or_else(|| Error::Internal("导入执行回执对应后台任务缺失".into()))?;
    validate_import_execution_replay(&fact, &batch, &job, receipt, prepared)?;
    Ok(Some(import_execution_result(
        ImportExecutionTransactionResult { batch, job, receipt: *receipt },
        fact.audit_event_id,
    )))
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
    executor: &mut dyn Executor,
) -> Result<ImportExecutionTransactionResult> {
    let mut batch = db
        .legacy_import_batches()
        .find_by_id(prepared.batch_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
    if !batch.has_version(prepared.expected_batch_version) {
        return Err(Error::ConflictError("导入批次版本已变化，请刷新后重试".to_string()));
    }
    let mut job = db
        .background_jobs()
        .find_by_request_id(&batch.batch_no, executor)
        .await?
        .ok_or_else(|| Error::Internal("导入批次后台任务缺失".to_string()))?;
    validate_import_background_job(&batch, &job)?;
    let confirmations = db.legacy_import_confirmations().list_by_batch(&prepared.batch_id, executor).await?;
    let trial_version = validate_import_execution_trial(prepared, &batch, &confirmations)?;
    let mut rows = if prepared.action == ImportExecutionAction::RetryFailed {
        db.legacy_import_rows().list_failed_by_batch(&prepared.batch_id, executor).await?
    } else {
        Vec::new()
    };
    let outcome = apply_import_execution_action(prepared, &mut batch, &mut job, &mut rows)?;
    if prepared.action == ImportExecutionAction::RetryFailed {
        db.legacy_import_rows().persist_failed_retry_rows(&mut rows, executor).await?;
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
        job_status: import_job_status(job.status),
        affected_items: outcome.affected_items,
        next_step: outcome.next_step,
    };
    Ok(ImportExecutionTransactionResult { batch, job, receipt })
}

/// 核对批次与后台任务的稳定关联和计数。
fn validate_import_background_job(batch: &LegacyImportBatch, job: &BackgroundJob) -> Result<()> {
    let matches = job.job_type == erp_support::JobType::Import
        && job.domain_job_type.as_deref() == Some(erp_support::LEGACY_IMPORT_DOMAIN_JOB_TYPE)
        && job.domain_job_id.as_deref() == Some(batch.base.id.as_str())
        && job.request_id == batch.batch_no
        && job.total_count == batch.total_rows;
    if matches {
        return Ok(());
    }
    Err(Error::BusinessLogicError("导入批次与后台任务关联不一致".to_string()))
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
        return Err(Error::ConflictError("导入试算版本已变化，请刷新后重试".to_string()));
    }
    if matches!(prepared.action, ImportExecutionAction::StartApply | ImportExecutionAction::RetryFailed) {
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
        return Err(Error::BusinessLogicError("当前试算尚未完成全部必要责任确认".to_string()));
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
        return Err(Error::BusinessLogicError("只有待应用批次与等待执行任务可以提交应用".to_string()));
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
        || !matches!(job.status, JobStatus::Pending | JobStatus::Running | JobStatus::PartiallySucceeded)
    {
        return Err(Error::BusinessLogicError("当前批次或后台任务状态不允许取消未应用项".to_string()));
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
        return Err(Error::BusinessLogicError("只有失败或部分失败批次可重新准备失败项".to_string()));
    }
    let retry_row_ids = rows
        .iter()
        .filter(|row| row.import_status == ImportStatus::Failed)
        .map(|row| row.base.id.clone())
        .collect::<BTreeSet<_>>();
    if retry_row_ids.is_empty() {
        return Err(Error::BusinessLogicError("当前批次没有可重试的失败项".to_string()));
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
/// * `action` - 稳定命令动作
/// * `command` - 已解析并规范化的执行命令
///
/// # 返回
/// 返回不暴露原始请求 ID 的命令 ID 与完整命令指纹。
fn import_execution_command_identity(
    actor_id: &str,
    action: &str,
    command: &PreparedImportExecution,
) -> LegacyImportCommandIdentity {
    let batch_version = command.expected_batch_version.to_string();
    let trial_version = command.expected_trial_version.map(|value| value.to_string()).unwrap_or_default();
    LegacyImportCommandIdentity::new(
        IMPORT_EXECUTION_COMMAND_PREFIX,
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

/// 核对幂等收据与当前批次/后台任务的稳定身份关联。
///
/// 当前版本与状态可能已被后台进度继续推进，重放必须返回收据记录的
/// 原始结果，不能要求当前快照仍停留在命令提交时刻。
fn validate_import_execution_replay(
    fact: &ImportCommandReceipt,
    batch: &LegacyImportBatch,
    job: &BackgroundJob,
    receipt: &ImportExecutionReceipt,
    prepared: &PreparedImportExecution,
) -> Result<()> {
    let ImportCommandResult::Execution { batch_id, background_job_id, .. } = &fact.result else {
        return Err(Error::ConflictError("导入执行回执结果类型不一致".into()));
    };
    validate_import_background_job(batch, job)?;
    if batch_id != &batch.base.id
        || background_job_id != &job.base.id
        || receipt.action != prepared.action
        || Some(receipt.batch_version) != prepared.expected_batch_version.checked_add(1)
        || receipt.trial_version != prepared.expected_trial_version
        || batch.base.version < receipt.batch_version
        || job.base.version < receipt.job_version
    {
        return Err(Error::ConflictError("导入执行回执与当前批次或后台任务身份不一致".into()));
    }
    Ok(())
}

/// 解析后台任务状态稳定码。
fn import_job_status(status: JobStatus) -> erp_import::ImportJobStatus {
    match status {
        JobStatus::Pending => erp_import::ImportJobStatus::Pending,
        JobStatus::Running => erp_import::ImportJobStatus::Running,
        JobStatus::PartiallySucceeded => erp_import::ImportJobStatus::PartiallySucceeded,
        JobStatus::Succeeded => erp_import::ImportJobStatus::Succeeded,
        JobStatus::Failed => erp_import::ImportJobStatus::Failed,
        JobStatus::Cancelled => erp_import::ImportJobStatus::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{ExternalIdentityMapId, LegacyImportBatchId, LegacyImportRowId, SourceSystemId};
    use erp_import::{LegacyImportBatchData, LegacyImportRowData, ParseStatus};

    use super::*;

    /// 经领域工厂登记测试后台任务（Service 只注入 ID 与发起人）。
    ///
    /// # 参数
    /// * `batch` - 导入批次实体
    /// * `actor` - 发起人账号 ID
    ///
    /// # 返回
    /// 返回新建的后台任务实体。
    fn test_background_job(batch: &LegacyImportBatch, actor: &str) -> erp_support::BackgroundJob {
        erp_support::BackgroundJob::for_legacy_import(
            erp_core::ids::BackgroundJobId::new(format!("job-{}", batch.batch_no)),
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
        row.mark_mapped(ExternalIdentityMapId::new(format!("mapping:{id}"))).unwrap();
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
        job.record_progress(1, 0, 0, Instant::from_unix_secs(1_700_000_100)).unwrap();

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
        job.record_import_result_batch(1, 1, 1, true, Instant::from_unix_secs(1_700_000_100)).unwrap();

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
        job.record_import_result_batch(0, 0, 1, false, Instant::from_unix_secs(1_700_000_100)).unwrap();
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
        job.record_import_result_batch(0, 0, 300, true, Instant::from_unix_secs(1_700_000_100)).unwrap();
        let outcome = prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).unwrap();
        assert_eq!(outcome.affected_items, 300);
        assert!(rows.iter().all(|row| row.import_status == ImportStatus::PendingImport));
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
        job.record_import_result_batch(2, 0, 1, true, Instant::from_unix_secs(1_700_000_100)).unwrap();
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
        job.record_import_result_batch(1, 0, 0, false, Instant::from_unix_secs(1_700_000_100)).unwrap();
        assert!(prepare_failed_import_retry(&mut import_batch, &mut job, &mut rows).is_err());
        assert_eq!(import_batch.success_rows, 1);
        assert_eq!(import_batch.failed_rows, 2);
    }

    fn execution_fact(
        command: &PreparedImportExecution,
        receipt: ImportExecutionReceipt,
        job_id: &str,
    ) -> ImportCommandReceipt {
        ImportCommandReceipt::new(
            import_execution_command_identity("user-1", "legacy_import_batch.execute", command)
                .structured_receipt("legacy_import_batch")
                .unwrap(),
            ImportCommandResult::Execution {
                batch_id: command.batch_id.to_string(),
                background_job_id: job_id.to_string(),
                receipt,
            },
            "event-1".into(),
        )
        .unwrap()
    }

    fn started_receipt() -> ImportExecutionReceipt {
        ImportExecutionReceipt {
            action: ImportExecutionAction::StartApply,
            result_status: ImportExecutionResultStatus::Started,
            batch_version: 5,
            batch_status: LegacyImportBatchStatus::Importing,
            trial_version: Some(2),
            job_version: 2,
            job_status: erp_import::ImportJobStatus::Running,
            affected_items: 1,
            next_step: ImportExecutionNextStep::MonitorProgress,
        }
    }

    #[test]
    fn execution_receipt_is_stable_and_rejects_request_reuse() {
        let command = execution_command(ImportExecutionAction::StartApply);
        let fact = execution_fact(&command, started_receipt(), "job-1");
        assert!(fact.ensure_identity(&fact.identity).is_ok());
        let mut changed = command;
        changed.expected_batch_version += 1;
        let identity = import_execution_command_identity("user-1", "legacy_import_batch.execute", &changed)
            .structured_receipt("legacy_import_batch")
            .unwrap();
        assert!(fact.ensure_identity(&identity).is_err());
        assert!(!fact.identity.command_id.contains("execution-1"));
        let decoded: ImportCommandReceipt =
            serde_json::from_str(&serde_json::to_string(&fact).unwrap()).unwrap();
        assert_eq!(decoded, fact);
        let mut damaged = decoded;
        damaged.schema_version = 99;
        assert!(damaged.ensure_identity(&identity).is_err());
    }

    #[test]
    fn execution_receipt_replay_allows_later_progress_but_rejects_different_job() {
        let command = execution_command(ImportExecutionAction::StartApply);
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::Completed;
        import_batch.base.version = 99;
        let mut job = test_background_job(&import_batch, "admin-1");
        job.base.version = 42;
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        let receipt = started_receipt();
        let fact = execution_fact(&command, receipt, &job.base.id);
        assert!(validate_import_execution_replay(&fact, &import_batch, &job, &receipt, &command).is_ok());
        let mut foreign_job = job.clone();
        foreign_job.base.id = "different-job".into();
        assert!(
            validate_import_execution_replay(&fact, &import_batch, &foreign_job, &receipt, &command).is_err()
        );
        let mut regressed = job;
        regressed.base.version = 1;
        assert!(
            validate_import_execution_replay(&fact, &import_batch, &regressed, &receipt, &command).is_err()
        );
    }
}
