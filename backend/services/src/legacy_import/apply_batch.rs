//! 导入批次逐行应用编排（INT-R29 / INT-E25 / INT-E27）。
//!
//! 请求 VO 在查库前拒绝重复 ID 与非法字段形状；仓储按请求行 ID 且受批次
//! 约束读取并报告缺失；未知 ID 失败关闭；仅合法状态迁移的 delta 行进入事务。

use std::collections::HashMap;

use database::{
    AccessControlExt, BulkJobExt, Executor, LegacyImportExt, NoTransaction, PartyExt, Transactional,
};
use entities::common::time::Instant;
use entities::legacy_import::{
    ApplyResultDraft, ApplyResultItem, ApplyResultOutcome, ApplyResultSet, ImportStatus, LegacyImportBatch,
    LegacyImportRow,
};
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    ApplyLegacyImportBatchRequest, ApplyRowOutcome, ApplyRowResult, LegacyImportBatchView,
    CUSTOMER_NOT_FOUND_ERROR_CODE, CUSTOMER_NOT_FOUND_ERROR_DETAIL, CUSTOMER_OBJECT_TYPE,
};
use super::LegacyImportService;

/// 本批实际发生合法状态迁移的行与计数。
struct ApplyBatchDeltas {
    rows: Vec<LegacyImportRow>,
    success: u64,
    failed: u64,
    skipped: u64,
}

impl ApplyBatchDeltas {
    /// 空增量。
    fn empty() -> Self {
        Self {
            rows: Vec::new(),
            success: 0,
            failed: 0,
            skipped: 0,
        }
    }
}

impl From<ApplyRowResult> for ApplyResultDraft {
    fn from(row: ApplyRowResult) -> Self {
        Self {
            row_id: row.row_id,
            outcome: match row.outcome {
                ApplyRowOutcome::Imported => ApplyResultOutcome::Imported,
                ApplyRowOutcome::Failed => ApplyResultOutcome::Failed,
                ApplyRowOutcome::Skipped => ApplyResultOutcome::Skipped,
            },
            external_identity_map_id: row.external_identity_map_id,
            target_document_id: row.target_document_id,
            target_object_reference: row.target_object_reference,
            error_code: row.error_code,
            error_detail: row.error_detail,
        }
    }
}

impl LegacyImportService {
    /// 应用导入批次（后台应用阶段逐行结果）。
    ///
    /// 批次处于 `Importing` 时执行：请求 VO 先拒绝重复 ID，仓储按请求 ID
    /// 且受批次约束读取，未知 ID 失败关闭；仅把实际发生合法状态迁移的
    /// delta 行、批次统计与后台任务进度写入同一事务。
    ///
    /// # 参数
    /// * `batch_id` - 导入批次 ID
    /// * `req` - 逐行结果
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回应用后批次的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `BusinessLogicError` - 批次未进入 `Importing` 阶段
    /// * `ValidationError` - 请求体校验失败或存在未知行 ID
    pub async fn apply_batch(
        &self,
        batch_id: &str,
        req: ApplyLegacyImportBatchRequest,
        actor: &AuditActor,
    ) -> Result<LegacyImportBatchView> {
        let result_set = apply_result_set_from_request(req)?;
        let mut batch = self
            .db
            .legacy_import_batches()
            .find_by_id(batch_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        if batch.is_terminal() {
            tracing::info!(batch_id, status = ?batch.status, "批次已终态，按幂等返回");
            return self.batch_view_of(batch).await;
        }
        if !batch.is_importing() {
            return Err(Error::BusinessLogicError(
                "批次尚未进入导入阶段，禁止应用".to_string(),
            ));
        }
        let job = self.load_running_import_job(&batch).await?;
        let scope_batch_id = entities::ids::LegacyImportBatchId::new(batch_id.to_string());
        let mut scope = self
            .db
            .legacy_import_rows()
            .apply_row_scope(&scope_batch_id, &result_set.row_ids(), &mut NoTransaction)
            .await?;
        ensure_no_missing_apply_rows(&scope.missing_row_ids)?;
        let party_ok = self.customer_import_party_ok(&result_set, &scope.rows).await?;
        let deltas = collect_apply_deltas(&result_set, &mut scope.rows, &party_ok)?;
        let pending_rows = scope.pending_outside_request;
        let all_terminal = pending_rows == 0;
        if deltas.rows.is_empty() && !all_terminal {
            return self.batch_view_of(batch).await;
        }
        advance_batch_counts(&mut batch, &deltas, pending_rows)?;
        self.persist_apply_batch(batch, job, deltas, all_terminal, actor)
            .await
    }

    /// 装载已由 `START_APPLY` 启动的后台任务。
    ///
    /// # 参数
    /// * `batch` - 导入中的批次
    ///
    /// # 返回
    /// 返回可记录进度的后台任务。
    ///
    /// # 错误
    /// 任务缺失或尚未启动时返回错误。
    async fn load_running_import_job(
        &self,
        batch: &LegacyImportBatch,
    ) -> Result<entities::bulk_job::BackgroundJob> {
        let job = self
            .db
            .background_jobs()
            .find_by_request_id(&batch.batch_no, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入批次后台任务缺失".to_string()))?;
        if !matches!(
            job.status,
            entities::bulk_job::JobStatus::Running | entities::bulk_job::JobStatus::PartiallySucceeded
        ) {
            return Err(Error::BusinessLogicError(
                "后台应用尚未由 START_APPLY 启动".to_string(),
            ));
        }
        Ok(job)
    }

    /// 预查客户行目标主体是否存在。
    ///
    /// 仅对仍待导入的 `CUSTOMER` 且 outcome 为 imported 的行查询 D07；
    /// 缺失记为 `false`，由 [`collect_apply_deltas`] 登记失败。
    ///
    /// # 参数
    /// * `result_set` - 已校验结果集
    /// * `rows` - 仓储命中行
    ///
    /// # 返回
    /// 返回行 ID → 主体是否存在；非客户行不出现，默认视为通过。
    ///
    /// # 错误
    /// 主体查询失败时返回错误。
    async fn customer_import_party_ok(
        &self,
        result_set: &ApplyResultSet,
        rows: &HashMap<String, LegacyImportRow>,
    ) -> Result<HashMap<String, bool>> {
        let mut hits = HashMap::new();
        for item in result_set.items() {
            let ApplyResultItem::Imported {
                target_document_id, ..
            } = item
            else {
                continue;
            };
            let Some(row) = rows.get(item.row_id().as_ref()) else {
                continue;
            };
            if row.import_status != ImportStatus::PendingImport
                || row.source_object_type != CUSTOMER_OBJECT_TYPE
            {
                continue;
            }
            let exists = self
                .db
                .parties()
                .find_by_id(target_document_id, &mut NoTransaction)
                .await?
                .is_some();
            hits.insert(row.base.id.clone(), exists);
        }
        Ok(hits)
    }

    /// 将 delta 行、批次、后台任务与审计写入同一事务。
    ///
    /// # 参数
    /// * `batch` - 已更新计数与状态的批次
    /// * `job` - 后台任务
    /// * `deltas` - 本批实际迁移的行
    /// * `all_terminal` - 是否全部行终态
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回提交后的批次视图。
    ///
    /// # 错误
    /// 事务写入或终态迁移失败时整体回滚。
    async fn persist_apply_batch(
        &self,
        batch: LegacyImportBatch,
        job: entities::bulk_job::BackgroundJob,
        deltas: ApplyBatchDeltas,
        all_terminal: bool,
        actor: &AuditActor,
    ) -> Result<LegacyImportBatchView> {
        let audit = actor.clone().resource_log(
            "legacy_import_batch.apply",
            "legacy_import_batch",
            batch.base.id.clone(),
        )?;
        let now = Instant::now();
        let db = self.db.clone();
        let client = db.client().clone();
        let mut rows_for_tx = deltas.rows;
        let mut batch_for_tx = batch;
        let mut job_for_tx = job;
        let success = deltas.success;
        let skipped = deltas.skipped;
        let failed = deltas.failed;
        let updated_batch = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    persist_apply_transaction(
                        PersistApplyWrite {
                            db: &db,
                            rows: &mut rows_for_tx,
                            batch: &mut batch_for_tx,
                            job: &mut job_for_tx,
                            audit: &audit,
                            success,
                            skipped,
                            failed,
                            all_terminal,
                            now,
                        },
                        session,
                    )
                    .await
                })
            })
            .await?;
        self.batch_view_of(updated_batch).await
    }
}

/// 将已校验结果应用到命中行，只收集实际发生合法迁移的 delta。
///
/// 已终态行跳过且不改字段；任一待导入行 `prepare_for_import` / `mark_*`
/// 失败则返回错误，调用方不得持久化本批 deltas。
///
/// # 参数
/// * `result_set` - 已通过唯一 ID 与字段形状校验的结果集
/// * `rows` - 仓储返回的命中行
/// * `party_ok` - 客户行主体存在性；缺项视为通过
///
/// # 返回
/// 返回需要交给 `update` 的 delta 行与本批计数。
///
/// # 错误
/// 行缺失或状态迁移非法时返回错误。
fn collect_apply_deltas(
    result_set: &ApplyResultSet,
    rows: &mut HashMap<String, LegacyImportRow>,
    party_ok: &HashMap<String, bool>,
) -> Result<ApplyBatchDeltas> {
    let mut deltas = ApplyBatchDeltas::empty();
    for item in result_set.items() {
        let row = rows
            .get_mut(item.row_id().as_ref())
            .ok_or_else(|| Error::Internal("应用范围缺失已确认存在的导入行".to_string()))?;
        if row.import_status != ImportStatus::PendingImport {
            continue;
        }
        let customer_exists = party_ok.get(&row.base.id).copied().unwrap_or(true);
        apply_pending_row(row, item, customer_exists)?;
        record_delta(&mut deltas, row, item);
    }
    Ok(deltas)
}

/// 对仍待导入的行执行一次合法状态迁移。
///
/// # 参数
/// * `row` - 待导入行
/// * `item` - 已校验的行结果
/// * `party_ok` - 客户行目标主体是否存在；非客户行传 `true`
///
/// # 错误
/// 行未准备好转导入或状态迁移失败时返回错误。
fn apply_pending_row(row: &mut LegacyImportRow, item: &ApplyResultItem, party_ok: bool) -> Result<()> {
    row.prepare_for_import(item.external_identity_map_id().cloned())?;
    match item {
        ApplyResultItem::Imported {
            target_document_id,
            target_object_reference,
            ..
        } => {
            if !party_ok {
                row.mark_import_failed(
                    CUSTOMER_NOT_FOUND_ERROR_CODE.to_string(),
                    Some(CUSTOMER_NOT_FOUND_ERROR_DETAIL.to_string()),
                )?;
                return Ok(());
            }
            row.mark_imported(target_document_id.clone(), target_object_reference.clone())?;
        }
        ApplyResultItem::Failed {
            error_code,
            error_detail,
            ..
        } => {
            row.mark_import_failed(error_code.clone(), error_detail.clone())?;
        }
        ApplyResultItem::Skipped {
            error_code,
            error_detail,
            ..
        } => {
            row.mark_skipped(error_code.clone(), error_detail.clone())?;
        }
    }
    Ok(())
}

/// 将 HTTP 请求收紧为应用结果集。
///
/// # 参数
/// * `req` - 原始应用请求
///
/// # 返回
/// 返回已拒绝重复 ID 与非法字段形状的结果集。
///
/// # 错误
/// 集合长度、重复 ID 或 outcome 字段形状非法时返回 `ValidationError`。
fn apply_result_set_from_request(req: ApplyLegacyImportBatchRequest) -> Result<ApplyResultSet> {
    req.validate()?;
    ApplyResultSet::try_from_drafts(req.results.into_iter().map(ApplyResultDraft::from).collect())
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 任一未知行 ID 失败关闭。
///
/// # 参数
/// * `missing` - 仓储返回的缺失 ID
///
/// # 错误
/// 存在缺失 ID 时返回 `ValidationError`。
fn ensure_no_missing_apply_rows(missing: &[entities::ids::LegacyImportRowId]) -> Result<()> {
    let Some(first) = missing.first() else {
        return Ok(());
    };
    Err(Error::ValidationError(format!(
        "导入行不属于当前批次或不存在: {first}"
    )))
}

/// 按本批 delta 更新批次计数并派生应用状态。
///
/// # 参数
/// * `batch` - 导入中的批次
/// * `deltas` - 本批实际迁移计数
/// * `pending_rows` - 请求外仍待导入的行数
///
/// # 错误
/// 计数不变量或状态迁移失败时返回错误。
fn advance_batch_counts(
    batch: &mut LegacyImportBatch,
    deltas: &ApplyBatchDeltas,
    pending_rows: u64,
) -> Result<()> {
    let success_rows = batch
        .success_rows
        .checked_add(deltas.success)
        .ok_or_else(|| Error::BusinessLogicError("成功行计数溢出".to_string()))?;
    let failed_rows = batch
        .failed_rows
        .checked_add(deltas.failed)
        .ok_or_else(|| Error::BusinessLogicError("失败行计数溢出".to_string()))?;
    batch.update_counts(batch.total_rows, success_rows, failed_rows)?;
    batch.advance(LegacyImportBatch::application_outcome(pending_rows, failed_rows))?;
    Ok(())
}

/// 将一次成功迁移记入 delta 集合。
///
/// # 参数
/// * `deltas` - 本批累计
/// * `row` - 已迁移的行
/// * `item` - 请求结果（客户主体缺失时 imported 记为失败）
fn record_delta(deltas: &mut ApplyBatchDeltas, row: &LegacyImportRow, item: &ApplyResultItem) {
    deltas.rows.push(row.clone());
    match item {
        ApplyResultItem::Imported { .. } if row.import_status == ImportStatus::Imported => {
            deltas.success += 1;
        }
        ApplyResultItem::Skipped { .. } => deltas.skipped += 1,
        ApplyResultItem::Failed { .. } | ApplyResultItem::Imported { .. } => deltas.failed += 1,
    }
}

/// 事务内导入应用写入所需的可变状态与本批计数。
struct PersistApplyWrite<'a> {
    /// 数据库。
    db: &'a Database,
    /// 本批 delta 行。
    rows: &'a mut [LegacyImportRow],
    /// 批次。
    batch: &'a mut LegacyImportBatch,
    /// 后台任务。
    job: &'a mut entities::bulk_job::BackgroundJob,
    /// 审计日志。
    audit: &'a entities::AuditLog,
    /// 本批成功数。
    success: u64,
    /// 本批跳过数。
    skipped: u64,
    /// 本批失败数。
    failed: u64,
    /// 是否全部行终态。
    all_terminal: bool,
    /// 进度时刻。
    now: Instant,
}

/// 事务内只更新 delta 行、批次、后台任务与审计。
///
/// # 参数
/// * `write` - 待持久化的 delta 行、批次、任务、审计与本批计数
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回已写入的批次实体。
///
/// # 错误
/// 任一行 CAS、任务终态或写入失败时返回错误，由调用方整体回滚。
///
/// # 约束
/// 不自行开启或提交事务；调用方必须传入同一事务 executor。
async fn persist_apply_transaction(
    write: PersistApplyWrite<'_>,
    executor: &mut dyn Executor,
) -> Result<LegacyImportBatch> {
    for row in write.rows.iter_mut() {
        write.db.legacy_import_rows().update(row, executor).await?;
    }
    write
        .db
        .legacy_import_batches()
        .update(write.batch, executor)
        .await?;
    write.job.record_import_result_batch(
        write.success,
        write.skipped,
        write.failed,
        write.all_terminal,
        write.now,
    )?;
    write.db.background_jobs().update(write.job, executor).await?;
    write.db.audit_logs().create(write.audit, executor).await?;
    Ok(write.batch.clone())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use entities::ids::{ExternalIdentityMapId, LegacyImportBatchId, LegacyImportRowId};
    use entities::legacy_import::{
        ApplyResultDraft, ApplyResultOutcome, ApplyResultSet, ImportStatus, LegacyImportRow,
        LegacyImportRowData, ParseStatus,
    };

    use super::{
        advance_batch_counts, apply_result_set_from_request, collect_apply_deltas,
        ensure_no_missing_apply_rows, persist_apply_transaction, PersistApplyWrite,
    };
    use crate::legacy_import::dto::{ApplyLegacyImportBatchRequest, ApplyRowOutcome, ApplyRowResult};

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

    fn draft(id: &str, outcome: ApplyResultOutcome) -> ApplyResultDraft {
        ApplyResultDraft {
            row_id: LegacyImportRowId::new(id),
            outcome,
            external_identity_map_id: None,
            target_document_id: None,
            target_object_reference: None,
            error_code: None,
            error_detail: None,
        }
    }

    fn imported_draft(id: &str, target: &str) -> ApplyResultDraft {
        ApplyResultDraft {
            target_document_id: Some(target.to_string()),
            ..draft(id, ApplyResultOutcome::Imported)
        }
    }

    fn failed_draft(id: &str, code: &str) -> ApplyResultDraft {
        ApplyResultDraft {
            error_code: Some(code.to_string()),
            ..draft(id, ApplyResultOutcome::Failed)
        }
    }

    fn skipped_draft(id: &str, code: &str) -> ApplyResultDraft {
        ApplyResultDraft {
            error_code: Some(code.to_string()),
            ..draft(id, ApplyResultOutcome::Skipped)
        }
    }

    fn imported_result(id: &str) -> ApplyRowResult {
        ApplyRowResult {
            row_id: LegacyImportRowId::new(id),
            outcome: ApplyRowOutcome::Imported,
            external_identity_map_id: None,
            target_document_id: Some("SO-1".to_string()),
            target_object_reference: None,
            error_code: None,
            error_detail: None,
        }
    }

    #[test]
    fn request_rejects_missing_and_forbidden_outcome_fields() {
        let missing_target = ApplyLegacyImportBatchRequest {
            results: vec![ApplyRowResult {
                target_document_id: None,
                ..imported_result("row-1")
            }],
        };
        assert!(apply_result_set_from_request(missing_target).is_err());

        let forbidden_error = ApplyLegacyImportBatchRequest {
            results: vec![ApplyRowResult {
                error_code: Some("X".to_string()),
                ..imported_result("row-1")
            }],
        };
        assert!(apply_result_set_from_request(forbidden_error).is_err());
    }

    #[test]
    fn request_rejects_duplicate_and_conflicting_ids_before_lookup() {
        let duplicate = ApplyLegacyImportBatchRequest {
            results: vec![imported_result("row-1"), imported_result("row-1")],
        };
        assert!(apply_result_set_from_request(duplicate).is_err());

        let conflict = ApplyLegacyImportBatchRequest {
            results: vec![
                imported_result("row-1"),
                ApplyRowResult {
                    outcome: ApplyRowOutcome::Failed,
                    target_document_id: None,
                    error_code: Some("X".to_string()),
                    ..imported_result("row-1")
                },
            ],
        };
        assert!(apply_result_set_from_request(conflict).is_err());
    }

    #[test]
    fn unknown_ids_fail_closed() {
        let missing = vec![LegacyImportRowId::new("row-unknown")];
        let error = ensure_no_missing_apply_rows(&missing).unwrap_err();
        assert!(error.to_string().contains("row-unknown"));
        assert!(ensure_no_missing_apply_rows(&[]).is_ok());
    }

    #[test]
    fn collect_skips_terminal_rows_and_only_returns_migrated_deltas() {
        let mut imported = applicable_row("row-imported");
        imported.mark_imported("SO-OLD".to_string(), None).unwrap();
        let mut failed = applicable_row("row-failed");
        failed.mark_import_failed("TEMPORARY".to_string(), None).unwrap();
        let mut skipped = applicable_row("row-skipped");
        skipped.mark_skipped("DUPLICATE".to_string(), None).unwrap();
        let pending = applicable_row("row-pending");
        let imported_version = imported.base.version;
        let imported_updated_at = imported.base.updated_at;
        let failed_version = failed.base.version;
        let skipped_version = skipped.base.version;
        let set = ApplyResultSet::try_from_drafts(vec![
            imported_draft("row-imported", "SO-NEW"),
            failed_draft("row-failed", "OTHER"),
            skipped_draft("row-skipped", "OTHER"),
            imported_draft("row-pending", "SO-1"),
        ])
        .unwrap();
        let mut rows = [imported.clone(), failed.clone(), skipped.clone(), pending]
            .into_iter()
            .map(|row| (row.base.id.clone(), row))
            .collect();

        let deltas = collect_apply_deltas(&set, &mut rows, &HashMap::new()).unwrap();

        assert_eq!(deltas.rows.len(), 1);
        assert_eq!(deltas.rows[0].base.id, "row-pending");
        assert_eq!(deltas.success, 1);
        assert_eq!(deltas.failed, 0);
        assert_eq!(deltas.skipped, 0);
        let imported = rows.get("row-imported").unwrap();
        assert_eq!(imported.import_status, ImportStatus::Imported);
        assert_eq!(imported.target_document_id.as_deref(), Some("SO-OLD"));
        assert_eq!(imported.base.version, imported_version);
        assert_eq!(imported.base.updated_at, imported_updated_at);
        let failed = rows.get("row-failed").unwrap();
        assert_eq!(failed.import_status, ImportStatus::Failed);
        assert_eq!(failed.base.version, failed_version);
        let skipped = rows.get("row-skipped").unwrap();
        assert_eq!(skipped.import_status, ImportStatus::Skipped);
        assert_eq!(skipped.base.version, skipped_version);
    }

    #[test]
    fn collect_fail_closed_does_not_return_deltas_when_later_row_is_illegal() {
        let pending = applicable_row("row-ok");
        let mut illegal = LegacyImportRow::new(
            LegacyImportRowId::new("row-illegal"),
            LegacyImportRowData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                source_object_type: "CONTRACT".to_string(),
                source_row_key: "row-illegal".to_string(),
                normalized_payload_reference: "payload:row-illegal".to_string(),
            },
        )
        .unwrap();
        illegal.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        illegal
            .mark_conflict("IDENTITY_CONFLICT".to_string(), None)
            .unwrap();
        let set = ApplyResultSet::try_from_drafts(vec![
            imported_draft("row-ok", "SO-1"),
            imported_draft("row-illegal", "SO-2"),
        ])
        .unwrap();
        let mut rows = [pending, illegal]
            .into_iter()
            .map(|row| (row.base.id.clone(), row))
            .collect();

        assert!(collect_apply_deltas(&set, &mut rows, &HashMap::new()).is_err());
    }

    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn persist_rolls_back_when_later_row_cas_fails() {
        use database::{
            ensure_indexes, AccessControlExt, BulkJobExt, LegacyImportExt, NoTransaction, Transactional,
        };
        use entities::bulk_job::{BackgroundJob, BackgroundJobData, JobStatus, JobType};
        use entities::common::time::{BusinessDate, Instant};
        use entities::ids::{BackgroundJobId, LegacyImportBatchId, SourceSystemId};
        use entities::legacy_import::{LegacyImportBatch, LegacyImportBatchData, LegacyImportBatchStatus};
        use entities::AccountKind;
        use entities::{AuditLog, AuditLogData};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("legacy_import_apply_cas")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let db = fixture.db();
            let mut batch = LegacyImportBatch::new(
                LegacyImportBatchId::new("batch-cas"),
                LegacyImportBatchData {
                    batch_no: "IMP-CAS".to_string(),
                    source_system_id: SourceSystemId::new("source-1"),
                    source_object_set: "CONTRACT".to_string(),
                    baseline_date: BusinessDate::from_ymd(2026, 8, 14).unwrap(),
                    import_rule_version: "rule-1".to_string(),
                    source_file_hmac: None,
                    status: LegacyImportBatchStatus::Importing,
                    total_rows: 2,
                    success_rows: 0,
                    failed_rows: 0,
                    failure_code_summary: None,
                    confirmation_status_summary: None,
                },
            )
            .unwrap();
            let mut job = BackgroundJob::new(
                BackgroundJobId::new("job-cas"),
                BackgroundJobData {
                    job_no: "BJ-IMP-CAS".to_string(),
                    job_type: JobType::Import,
                    domain_job_type: Some("LEGACY_IMPORT".to_string()),
                    domain_job_id: Some(batch.base.id.clone()),
                    selection_snapshot_id: None,
                    requested_by: "admin-1".to_string(),
                    request_id: batch.batch_no.clone(),
                    input_file_asset_id: None,
                    result_file_asset_id: None,
                    total_count: 2,
                },
            )
            .unwrap();
            job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
            let pending_ok = applicable_row("row-ok");
            let pending_stale = applicable_row("row-stale");
            db.legacy_import_batches()
                .create(&batch, &mut NoTransaction)
                .await
                .expect("批次写入失败");
            db.background_jobs()
                .create(&job, &mut NoTransaction)
                .await
                .expect("任务写入失败");
            db.legacy_import_rows()
                .create(&pending_ok, &mut NoTransaction)
                .await
                .expect("行写入失败");
            db.legacy_import_rows()
                .create(&pending_stale, &mut NoTransaction)
                .await
                .expect("冲突行写入失败");

            let set = ApplyResultSet::try_from_drafts(vec![
                imported_draft("row-ok", "SO-1"),
                imported_draft("row-stale", "SO-2"),
            ])
            .unwrap();
            let mut rows = [pending_ok, pending_stale]
                .into_iter()
                .map(|row| (row.base.id.clone(), row))
                .collect();
            let mut deltas = collect_apply_deltas(&set, &mut rows, &HashMap::new()).unwrap();
            let mut bumped = db
                .legacy_import_rows()
                .find_by_id("row-stale", &mut NoTransaction)
                .await
                .expect("重读冲突行失败")
                .expect("冲突行必须存在");
            db.legacy_import_rows()
                .update(&mut bumped, &mut NoTransaction)
                .await
                .expect("抬升冲突行版本失败");
            advance_batch_counts(&mut batch, &deltas, 0).unwrap();
            let audit = AuditLog::new(
                "audit-cas-1".to_string(),
                AuditLogData {
                    actor_id: "actor-1".to_string(),
                    actor_account: "admin01".to_string(),
                    actor_type: AccountKind::Admin,
                    action: "legacy_import_batch.apply".to_string(),
                    resource_type: "legacy_import_batch".to_string(),
                    resource_id: Some(batch.base.id.clone()),
                    success: true,
                    message: None,
                },
            )
            .unwrap();
            let client = db.client().clone();
            let persist_db = db.clone();
            let persist: crate::errors::Result<LegacyImportBatch> = client
                .with_transaction(move |session| {
                    Box::pin(async move {
                        persist_apply_transaction(
                            PersistApplyWrite {
                                db: &persist_db,
                                rows: &mut deltas.rows,
                                batch: &mut batch,
                                job: &mut job,
                                audit: &audit,
                                success: 2,
                                skipped: 0,
                                failed: 0,
                                all_terminal: true,
                                now: Instant::from_unix_secs(1_700_000_100),
                            },
                            session,
                        )
                        .await
                    })
                })
                .await;
            assert!(persist.is_err(), "后行乐观锁失败必须整体回滚");

            let ok = db
                .legacy_import_rows()
                .find_by_id("row-ok", &mut NoTransaction)
                .await
                .unwrap()
                .unwrap();
            let stale = db
                .legacy_import_rows()
                .find_by_id("row-stale", &mut NoTransaction)
                .await
                .unwrap()
                .unwrap();
            let stored_batch = db
                .legacy_import_batches()
                .find_by_id("batch-cas", &mut NoTransaction)
                .await
                .unwrap()
                .unwrap();
            let stored_job = db
                .background_jobs()
                .find_by_id("job-cas", &mut NoTransaction)
                .await
                .unwrap()
                .unwrap();
            let stored_audit = db
                .audit_logs()
                .find_by_id("audit-cas-1", &mut NoTransaction)
                .await
                .unwrap();
            assert_eq!(ok.import_status, ImportStatus::PendingImport);
            assert_eq!(stale.import_status, ImportStatus::PendingImport);
            assert_eq!(stored_batch.status, LegacyImportBatchStatus::Importing);
            assert_eq!(stored_batch.success_rows, 0);
            assert_eq!(stored_job.status, JobStatus::Running);
            assert_eq!(stored_job.processed_count, 0);
            assert!(stored_audit.is_none());
        });
    }
}
