//! 域 D22 `legacy_import` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建批次（批次 + 来源行 + D04 后台任务 + 审计日志）→ 跨集合，
//!   `database::Transactional::with_transaction` 内经 `LegacyImportRepository::create_batch_with_rows`
//!   与 D04/D02 仓储写入，保证「批次 + 明细 + 后台任务」原子可见；
//! - 创建/决策确认事实（确认 + 批次状态推进 + 审计日志）→ 跨集合事务；
//! - 批次应用（行状态推进 + 批次统计 + 后台任务进度 + 审计日志）→ 跨集合事务；
//! - 其余单集合查询传 `&mut NoTransaction`。
//!
//! 跨域协作只经 `DatabaseExt` 调对方 Repository（P3-service-api §2）：
//! - D04 `bulk_job`：登记/推进 `background_job`（批次导入的后台任务）；
//! - D05 `file_asset`：批次创建时校验资产引用存在；
//! - D07 `party`：客户行导入前校验目标主体存在（`CUSTOMER_NOT_FOUND`）。
//!
//! 幂等约定（AGENTS.md 外部依赖容错）：批次号、`(batch_id, scope, trial_version)`
//! 唯一索引为权威去重；重复提交返回既有事实，不产生重复正式记录。

use database::{
    AccessControlExt, BulkJobExt, FileAssetExt, LegacyImportExt, NoTransaction, PartyExt, Transactional,
};
use entities::common::time::Instant;
use entities::legacy_import::{
    ConfirmationDecision, ConfirmationStatus, ImportStatus, LegacyImportBatch, LegacyImportBatchId,
    LegacyImportBatchStatus, LegacyImportConfirmation, LegacyImportConfirmationData,
    LegacyImportConfirmationId, LegacyImportRow, LegacyImportRowData, LegacyImportRowId, ParseStatus,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub mod dto;

use self::dto::{
    build_background_job, ApplyRowOutcome, LegacyImportBatchListQuery, SortDir,
    CUSTOMER_NOT_FOUND_ERROR_CODE, CUSTOMER_NOT_FOUND_ERROR_DETAIL, CUSTOMER_OBJECT_TYPE,
};
pub use self::dto::{
    ApplyLegacyImportBatchRequest, ApplyRowResult, CreateLegacyImportBatchRequest,
    CreateLegacyImportConfirmationRequest, DecideLegacyImportConfirmationRequest, ImportRowRequest,
    LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchView,
    LegacyImportConfirmationListParams, LegacyImportConfirmationView, LegacyImportRowListParams,
    LegacyImportRowView, PageView,
};

/// 导入批次列表筛选条件类型（经 `LegacyImportExt` 关联类型跨 crate 可达）。
type LegacyImportBatchFilter = <mongodb::Database as LegacyImportExt>::LegacyImportBatchFilter;
/// 导入行列表筛选条件类型。
type LegacyImportRowFilter = <mongodb::Database as LegacyImportExt>::LegacyImportRowFilter;
/// 导入确认列表筛选条件类型。
type LegacyImportConfirmationFilter = <mongodb::Database as LegacyImportExt>::LegacyImportConfirmationFilter;

/// 旧数据导入服务。
///
/// 提供导入批次、导入行与业务确认事实的创建、查询与状态推进编排。
pub struct LegacyImportService {
    db: Database,
}

impl LegacyImportService {
    /// 创建旧数据导入服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 创建导入批次（批次 + 来源行 + 后台任务原子写入）。
    ///
    /// 批次号唯一：重复提交按幂等处理，直接返回既有批次（不产生重复事实）。
    /// 资产引用（成功包/manifest/失败诊断包）存在性经 D05 仓储校验，
    /// 后台任务经 D04 仓储与批次同一事务登记。
    ///
    /// # 参数
    /// * `req` - 创建请求（批次头 + 来源行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或既有）批次的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 资产引用不存在
    /// * `ValidationError` - 请求体校验失败
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_batch(
        &self,
        req: CreateLegacyImportBatchRequest,
        actor: &AuditActor,
    ) -> Result<LegacyImportBatchView> {
        req.validate()?;
        if let Some(existing) = self
            .db
            .legacy_import_batches()
            .find_by_batch_no(&req.batch_no, &mut NoTransaction)
            .await?
        {
            tracing::info!(batch_no = %req.batch_no, "批次已存在，按幂等返回既有批次");
            return self.batch_view_of(existing).await;
        }
        self.ensure_file_assets_exist(&req).await?;

        let id = LegacyImportBatchId::new(next_id());
        let rows = self.build_rows(&req, &id)?;
        let batch = LegacyImportBatch::new(
            id.clone(),
            entities::legacy_import::LegacyImportBatchData {
                batch_no: req.batch_no,
                source_system_id: req.source_system_id,
                source_object_set: req.source_object_set,
                baseline_date: req.baseline_date,
                import_rule_version: req.import_rule_version,
                source_file_hmac: req.source_file_hmac,
                status: LegacyImportBatchStatus::PendingValidation,
                total_rows: rows.len() as u64,
                success_rows: 0,
                failed_rows: 0,
                failure_code_summary: None,
                confirmation_status_summary: None,
            },
        )?;
        let background_job = build_background_job(&batch, actor.id())?;
        let audit = actor.clone().resource_log(
            "legacy_import_batch.create",
            "legacy_import_batch",
            id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let batch_for_tx = batch.clone();
        let rows_for_tx = rows.clone();
        let job_for_tx = background_job.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.legacy_import()
                        .create_batch_with_rows(&batch_for_tx, &rows_for_tx, session)
                        .await?;
                    db.background_jobs().create(&job_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        let mut view: LegacyImportBatchView = batch.into();
        view.background_job_id = Some(background_job.base.id);
        Ok(view)
    }

    /// 分页查询导入批次列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn batch_list(
        &self,
        params: &LegacyImportBatchListParams,
    ) -> Result<PageView<LegacyImportBatchListItem>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = self.batch_filter_of(&query);
        let page = self
            .db
            .legacy_import_batches()
            .search_legacy_import_batches(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| LegacyImportBatchListItem {
                id: row.id,
                batch_no: row.batch_no,
                source_system_id: row.source_system_id.to_string(),
                source_object_set: row.source_object_set,
                baseline_date: row.baseline_date,
                import_rule_version: row.import_rule_version,
                status: row.status,
                total_rows: row.total_rows,
                success_rows: row.success_rows,
                failed_rows: row.failed_rows,
                failure_code_summary: row.failure_code_summary,
                confirmation_status_summary: row.confirmation_status_summary,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询导入批次详情（含后台任务关联）。
    ///
    /// # 参数
    /// * `id` - 导入批次 ID
    ///
    /// # 返回
    /// 返回批次的响应视图（含 `background_job_id`）。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn batch_detail(&self, id: &str) -> Result<LegacyImportBatchView> {
        let batch = self
            .db
            .legacy_import_batches()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        self.batch_view_of(batch).await
    }

    /// 分页查询导入行列表（按批次）。
    ///
    /// # 参数
    /// * `batch_id` - 所属导入批次
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn row_list(
        &self,
        batch_id: &str,
        params: &LegacyImportRowListParams,
    ) -> Result<PageView<LegacyImportRowView>> {
        self.ensure_batch_exists(batch_id).await?;
        params.validate()?;
        let query = params.normalized()?;
        let filter = LegacyImportRowFilter {
            batch_id: Some(LegacyImportBatchId::new(batch_id.to_string())),
            parse_status: query.parse_status,
            mapping_status: query.mapping_status,
            import_status: query.import_status,
            source_row_key: query.source_row_key,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .legacy_import_rows()
            .search_legacy_import_rows(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| LegacyImportRowView {
                id: row.id,
                batch_id: row.batch_id.to_string(),
                source_object_type: row.source_object_type,
                source_row_key: row.source_row_key,
                parse_status: row.parse_status,
                mapping_status: row.mapping_status,
                import_status: row.import_status,
                external_identity_map_id: row.external_identity_map_id.map(|id| id.to_string()),
                error_code: row.error_code,
                target_document_id: row.target_document_id,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询导入确认事实列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`batch_id` 为主要筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn confirmation_list(
        &self,
        params: &LegacyImportConfirmationListParams,
    ) -> Result<PageView<LegacyImportConfirmationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = LegacyImportConfirmationFilter {
            batch_id: query.batch_id,
            confirmation_scope: query.confirmation_scope,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .legacy_import_confirmations()
            .search_legacy_import_confirmations(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| LegacyImportConfirmationView {
                id: row.id,
                batch_id: row.batch_id.to_string(),
                confirmation_scope: row.confirmation_scope,
                owner_role: row.owner_role,
                batch_version: row.batch_version,
                trial_version: row.trial_version,
                status: row.status,
                decision: row.decision,
                reason_code: row.reason_code,
                comment: None,
                work_item_id: row.work_item_id.to_string(),
                decided_by: row.decided_by,
                decided_at: row.decided_at.map(|at| at as i64),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建待确认确认事实。
    ///
    /// 批次推进到 `PendingConfirmation`（试算完成）；同一
    /// `(batch_id, scope, trial_version)` 重复提交按幂等返回既有事实。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或既有）确认事实的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `BusinessLogicError` - 批次已进入不可确认阶段
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_confirmation(
        &self,
        req: CreateLegacyImportConfirmationRequest,
        actor: &AuditActor,
    ) -> Result<LegacyImportConfirmationView> {
        req.validate()?;
        if let Some(existing) = self
            .db
            .legacy_import_confirmations()
            .find_by_batch_scope_trial(
                &req.batch_id,
                &req.confirmation_scope,
                req.trial_version,
                &mut NoTransaction,
            )
            .await?
        {
            tracing::info!(batch_id = %req.batch_id, scope = %req.confirmation_scope, "确认事实已存在，按幂等返回");
            return Ok(existing.into());
        }
        let mut batch = self
            .db
            .legacy_import_batches()
            .find_by_id(req.batch_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        Self::advance_batch_to_pending_confirmation(&mut batch)?;

        let confirmation = LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new(next_id()),
            LegacyImportConfirmationData {
                batch_id: req.batch_id,
                confirmation_scope: req.confirmation_scope,
                owner_role: req.owner_role,
                batch_version: req.batch_version,
                trial_version: req.trial_version,
                import_rule_version: req.import_rule_version,
                work_item_id: req.work_item_id,
            },
        )?;
        let audit = actor.clone().resource_log(
            "legacy_import_confirmation.create",
            "legacy_import_confirmation",
            confirmation.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let confirmation_for_tx = confirmation.clone();
        let mut batch_for_tx = batch.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.legacy_import_confirmations()
                        .create(&confirmation_for_tx, session)
                        .await?;
                    db.legacy_import_batches()
                        .update(&mut batch_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<LegacyImportConfirmation, crate::errors::Error>(confirmation_for_tx)
                })
            })
            .await?;

        Ok(confirmation.into())
    }

    /// 决策确认事实（`CONFIRM_SCOPE` 或 `RETURN_FOR_FIX`）。
    ///
    /// 幂等：同一确认事实重复提交相同决策直接返回既有结论（不产生重复事实）；
    /// 已处理但决策不一致返回 409。`CONFIRM_SCOPE` 且全部必要范围确认完成时，
    /// 批次在同一事务内推进到 `Importing`（生产应用 guard 通过，§6.12）。
    ///
    /// # 参数
    /// * `id` - 确认事实 ID
    /// * `req` - 决策请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回决策后确认事实的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 确认事实不存在
    /// * `ConflictError` - 已处理且决策不一致
    /// * `ValidationError` - 请求体校验失败（退回缺少原因代码等）
    pub async fn decide_confirmation(
        &self,
        id: &str,
        req: DecideLegacyImportConfirmationRequest,
        actor: &AuditActor,
    ) -> Result<LegacyImportConfirmationView> {
        req.validate()?;
        let mut confirmation = self
            .db
            .legacy_import_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入确认事实不存在".to_string()))?;
        if Self::already_decided(&confirmation, req.decision)? {
            return Ok(confirmation.into());
        }

        confirmation.decide(
            req.decision,
            actor.id(),
            Instant::now(),
            req.reason_code,
            req.comment,
        )?;
        let batch_advance = if req.decision == ConfirmationDecision::ConfirmScope {
            self.confirm_matrix_complete(&confirmation.batch_id, &confirmation.base.id)
                .await?
        } else {
            None
        };
        let audit = actor.clone().resource_log(
            "legacy_import_confirmation.decide",
            "legacy_import_confirmation",
            confirmation.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut confirmation_for_tx = confirmation.clone();
        let mut batch_for_tx = batch_advance;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.legacy_import_confirmations()
                        .update(&mut confirmation_for_tx, session)
                        .await?;
                    if let Some(batch) = batch_for_tx.as_mut() {
                        db.legacy_import_batches().update(batch, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<LegacyImportConfirmation, crate::errors::Error>(confirmation_for_tx)
                })
            })
            .await?;

        Ok(confirmation.into())
    }

    /// 应用导入批次（后台应用阶段逐行结果）。
    ///
    /// 批次处于 `Importing` 时执行：逐行推进解析/映射/导入状态、更新批次统计
    /// 与状态、推进 D04 后台任务进度；客户行导入前经 D07 校验目标主体存在。
    /// 幂等：批次已终态或行已终态时重复提交不产生新写入。
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
    /// * `ValidationError` - 请求体校验失败
    pub async fn apply_batch(
        &self,
        batch_id: &str,
        req: ApplyLegacyImportBatchRequest,
        actor: &AuditActor,
    ) -> Result<LegacyImportBatchView> {
        req.validate()?;
        let mut batch = self
            .db
            .legacy_import_batches()
            .find_by_id(batch_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        if Self::batch_terminal(batch.status) {
            tracing::info!(batch_id, status = ?batch.status, "批次已终态，按幂等返回");
            return self.batch_view_of(batch).await;
        }
        if batch.status != LegacyImportBatchStatus::Importing {
            return Err(Error::BusinessLogicError(
                "批次尚未进入导入阶段，禁止应用".to_string(),
            ));
        }
        let mut rows = self
            .db
            .legacy_import_rows()
            .find_rows_by_batch_ids(
                &[LegacyImportBatchId::new(batch_id.to_string())],
                &mut NoTransaction,
            )
            .await?;
        let background_job = self
            .db
            .background_jobs()
            .find_by_request_id(&batch.batch_no, &mut NoTransaction)
            .await?;

        let now = Instant::now();
        let (mut delta_success, mut delta_failed, mut delta_skipped) = (0u64, 0u64, 0u64);
        for result in &req.results {
            let Some(row) = rows
                .iter_mut()
                .find(|row| row.base.id == result.row_id.to_string())
            else {
                continue;
            };
            if row.import_status != ImportStatus::PendingImport {
                continue;
            }
            Self::advance_row_to_applicable(row, result)?;
            match result.outcome {
                ApplyRowOutcome::Imported => {
                    let target = result.target_document_id.clone().ok_or_else(|| {
                        Error::ValidationError("导入成功结果必须提供目标单据 ID".to_string())
                    })?;
                    if self.party_validates_for(row, &target).await? {
                        row.mark_imported(target, result.target_object_reference.clone())?;
                        delta_success += 1;
                    } else {
                        delta_failed += 1;
                    }
                }
                ApplyRowOutcome::Failed => {
                    let code = result
                        .error_code
                        .clone()
                        .ok_or_else(|| Error::ValidationError("失败结果必须提供错误码".to_string()))?;
                    row.mark_import_failed(code, result.error_detail.clone())?;
                    delta_failed += 1;
                }
                ApplyRowOutcome::Skipped => {
                    let code = result
                        .error_code
                        .clone()
                        .ok_or_else(|| Error::ValidationError("跳过结果必须提供原因错误码".to_string()))?;
                    row.mark_skipped(code, result.error_detail.clone())?;
                    delta_skipped += 1;
                }
            }
        }

        let success_rows = count_by_status(&rows, ImportStatus::Imported);
        let failed_rows = count_by_status(&rows, ImportStatus::Failed);
        batch.update_counts(batch.total_rows, success_rows, failed_rows)?;
        let all_terminal = count_pending(&rows) == 0;
        let outcome = if all_terminal && failed_rows == 0 {
            LegacyImportBatchStatus::Completed
        } else if all_terminal {
            LegacyImportBatchStatus::PartialFailed
        } else {
            LegacyImportBatchStatus::Importing
        };
        batch.advance(outcome)?;
        let audit = actor.clone().resource_log(
            "legacy_import_batch.apply",
            "legacy_import_batch",
            batch.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut rows_for_tx = rows.clone();
        let mut batch_for_tx = batch.clone();
        let mut job_for_tx = background_job.clone();
        let updated_batch = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    for row in &mut rows_for_tx {
                        if row.import_status != ImportStatus::PendingImport {
                            db.legacy_import_rows().update(row, session).await?;
                        }
                    }
                    db.legacy_import_batches()
                        .update(&mut batch_for_tx, session)
                        .await?;
                    if let Some(job) = job_for_tx.as_mut() {
                        Self::advance_background_job(
                            job,
                            delta_success,
                            delta_skipped,
                            delta_failed,
                            all_terminal,
                            now,
                        )?;
                        db.background_jobs().update(job, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<LegacyImportBatch, crate::errors::Error>(batch_for_tx)
                })
            })
            .await?;

        self.batch_view_of(updated_batch).await
    }

    /// 推进后台任务进度并收尾。
    ///
    /// 任务为 `PENDING` 时先启动；按本批结果累加进度；全部行终态时
    /// 标记成功（失败数为零）或部分成功（存在失败）。
    ///
    /// # 参数
    /// * `job` - 后台任务（内存态）
    /// * `success` - 本批成功数
    /// * `skipped` - 本批跳过数
    /// * `failed` - 本批失败数
    /// * `all_terminal` - 是否全部行终态
    /// * `at` - 当前时刻
    ///
    /// # 错误
    /// 状态迁移或计数不变量违反时返回错误。
    fn advance_background_job(
        job: &mut entities::bulk_job::BackgroundJob,
        success: u64,
        skipped: u64,
        failed: u64,
        all_terminal: bool,
        at: Instant,
    ) -> Result<()> {
        if job.status == entities::bulk_job::JobStatus::Pending {
            job.start(at)?;
        }
        if success + skipped + failed > 0 {
            job.record_progress(success, skipped, failed, at)?;
        }
        if all_terminal {
            if failed == 0 {
                job.mark_succeeded(at)?;
            } else {
                job.mark_partially_succeeded()?;
                job.mark_succeeded(at)?;
            }
        }
        Ok(())
    }

    /// 推进行到可导入状态（解析有效 + 映射完成）。
    ///
    /// 行处于待解析时标记有效；待映射时登记来源稳定身份（`external_identity_map_id`
    /// 由试算阶段在 D01 建立，此处只登记）；已无效的行无法应用。
    ///
    /// # 参数
    /// * `row` - 导入行（内存态）
    /// * `result` - 行级结果请求
    ///
    /// # 错误
    /// 行已无效，或映射身份缺失时返回错误。
    fn advance_row_to_applicable(row: &mut LegacyImportRow, result: &ApplyRowResult) -> Result<()> {
        if row.parse_status == ParseStatus::PendingParse {
            row.mark_parse_result(ParseStatus::Valid, None, None)?;
        }
        if row.parse_status != ParseStatus::Valid {
            return Err(Error::BusinessLogicError("无效行不能进入导入".to_string()));
        }
        if row.mapping_status == entities::legacy_import::MappingStatus::PendingMapping {
            let identity = result
                .external_identity_map_id
                .clone()
                .ok_or_else(|| Error::ValidationError("待映射行必须提供来源稳定身份".to_string()))?;
            row.mark_mapped(identity)?;
        }
        Ok(())
    }

    /// 校验客户行目标主体存在（D07 仓储读取）。
    ///
    /// 客户行（`CUSTOMER`）必须命中 D07 既有主体；未命中时行标记失败
    /// `CUSTOMER_NOT_FOUND` 并返回 `Ok(false)`（按失败处理）。
    ///
    /// # 参数
    /// * `row` - 导入行（内存态）
    /// * `target_document_id` - 目标单据 ID（客户行应为主体 ID）
    ///
    /// # 返回
    /// 校验通过返回 `Ok(true)`；主体缺失时返回 `Ok(false)`（行已标记失败）。
    ///
    /// # 错误
    /// 行状态迁移或数据库读取失败时返回错误。
    async fn party_validates_for(&self, row: &mut LegacyImportRow, target_document_id: &str) -> Result<bool> {
        if row.source_object_type != CUSTOMER_OBJECT_TYPE {
            return Ok(true);
        }
        let exists = self
            .db
            .parties()
            .find_by_id(target_document_id, &mut NoTransaction)
            .await?
            .is_some();
        if exists {
            return Ok(true);
        }
        row.mark_import_failed(
            CUSTOMER_NOT_FOUND_ERROR_CODE.to_string(),
            Some(CUSTOMER_NOT_FOUND_ERROR_DETAIL.to_string()),
        )?;
        Ok(false)
    }

    /// 查询批次全部确认事实并判定确认矩阵是否全绿。
    ///
    /// 全绿时返回待推进到 `Importing` 的批次实体（内存态），否则返回 `None`。
    /// 本次已决策的确认事实尚未落库，按已确认口径参与判定（其 `status`
    /// 已在内存中推进为 `Confirmed`）。
    ///
    /// # 参数
    /// * `batch_id` - 导入批次 ID
    /// * `decided_confirmation_id` - 本次已决策的确认事实 ID
    ///
    /// # 返回
    /// 返回待推进的批次实体（可能为 `None`）。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `RepositoryError` - 数据库查询失败
    async fn confirm_matrix_complete(
        &self,
        batch_id: &LegacyImportBatchId,
        decided_confirmation_id: &str,
    ) -> Result<Option<LegacyImportBatch>> {
        let confirmations = self
            .db
            .legacy_import_confirmations()
            .find_many_sorted(
                mongodb::bson::doc! { "batch_id": batch_id.to_string() },
                mongodb::bson::doc! { "created_at": 1 },
                &mut NoTransaction,
            )
            .await?;
        if confirmations.is_empty()
            || confirmations.iter().any(|confirmation| {
                confirmation.status != ConfirmationStatus::Confirmed
                    && confirmation.base.id != decided_confirmation_id
            })
        {
            return Ok(None);
        }
        let mut batch = self
            .db
            .legacy_import_batches()
            .find_by_id(batch_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        if batch.status != LegacyImportBatchStatus::PendingConfirmation {
            return Ok(None);
        }
        batch.advance(LegacyImportBatchStatus::Importing)?;
        Ok(Some(batch))
    }

    /// 判定确认事实是否已按相同决策处理（幂等返回）。
    ///
    /// # 参数
    /// * `confirmation` - 确认事实
    /// * `decision` - 请求决策
    ///
    /// # 返回
    /// 相同决策已处理返回 `Ok(true)`；已处理但决策不一致返回 `Err(ConflictError)`。
    fn already_decided(
        confirmation: &LegacyImportConfirmation,
        decision: ConfirmationDecision,
    ) -> Result<bool> {
        if confirmation.status == ConfirmationStatus::Pending {
            return Ok(false);
        }
        let same = (confirmation.status == ConfirmationStatus::Confirmed
            && decision == ConfirmationDecision::ConfirmScope)
            || (confirmation.status == ConfirmationStatus::Rejected
                && decision == ConfirmationDecision::ReturnForFix);
        if same {
            return Ok(true);
        }
        Err(Error::ConflictError("确认事实已处理，不能重复决策".to_string()))
    }

    /// 推进批次到待确认阶段（试算完成）。
    ///
    /// # 参数
    /// * `batch` - 导入批次（内存态，成功后状态为 `PendingConfirmation`）
    ///
    /// # 错误
    /// 批次已离开可确认阶段时返回错误。
    fn advance_batch_to_pending_confirmation(batch: &mut LegacyImportBatch) -> Result<()> {
        match batch.status {
            LegacyImportBatchStatus::PendingValidation => {
                batch.advance(LegacyImportBatchStatus::Validating)?;
                batch.advance(LegacyImportBatchStatus::PendingConfirmation)?;
                Ok(())
            }
            LegacyImportBatchStatus::Validating => {
                batch.advance(LegacyImportBatchStatus::PendingConfirmation)?;
                Ok(())
            }
            LegacyImportBatchStatus::PendingConfirmation => Ok(()),
            _ => Err(Error::BusinessLogicError(
                "批次已离开可确认阶段，禁止创建确认事实".to_string(),
            )),
        }
    }

    /// 判定批次状态是否终态。
    ///
    /// # 参数
    /// * `status` - 批次状态
    ///
    /// # 返回
    /// `Completed`/`PartialFailed`/`Failed` 时返回 `true`。
    fn batch_terminal(status: LegacyImportBatchStatus) -> bool {
        matches!(
            status,
            LegacyImportBatchStatus::Completed
                | LegacyImportBatchStatus::PartialFailed
                | LegacyImportBatchStatus::Failed
        )
    }

    /// 构造导入批次列表筛选条件。
    ///
    /// # 参数
    /// * `query` - 归一化查询参数
    ///
    /// # 返回
    /// 返回仓储筛选条件。
    fn batch_filter_of(&self, query: &LegacyImportBatchListQuery) -> LegacyImportBatchFilter {
        LegacyImportBatchFilter {
            batch_no: query.batch_no.clone(),
            source_system_id: query.source_system_id.clone(),
            status: query.status,
            baseline_date_from: query.baseline_date_from,
            baseline_date_to: query.baseline_date_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        }
    }

    /// 构造导入批次详情视图（补充 D04 后台任务关联）。
    ///
    /// # 参数
    /// * `batch` - 导入批次实体
    ///
    /// # 返回
    /// 返回含 `background_job_id` 的响应视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn batch_view_of(&self, batch: LegacyImportBatch) -> Result<LegacyImportBatchView> {
        let background_job = self
            .db
            .background_jobs()
            .find_by_request_id(&batch.batch_no, &mut NoTransaction)
            .await?;
        let mut view: LegacyImportBatchView = batch.into();
        view.background_job_id = background_job.map(|job| job.base.id);
        Ok(view)
    }

    /// 校验批次引用的资产存在（D05 仓储读取）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    ///
    /// # 错误
    /// * `NotFound` - 资产引用不存在
    async fn ensure_file_assets_exist(&self, req: &CreateLegacyImportBatchRequest) -> Result<()> {
        for (label, asset_id) in [
            ("成功白名单包", req.successful_sanitized_file_asset_id.as_ref()),
            ("成功 manifest", req.success_manifest_file_asset_id.as_ref()),
            ("失败诊断包", req.failure_diagnostic_file_asset_id.as_ref()),
        ] {
            if let Some(asset_id) = asset_id {
                if self
                    .db
                    .file_assets()
                    .find_by_id(asset_id.as_ref(), &mut NoTransaction)
                    .await?
                    .is_none()
                {
                    return Err(Error::NotFound(format!("{label}资产不存在")));
                }
            }
        }
        Ok(())
    }

    /// 构造导入行实体列表。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `batch_id` - 所属导入批次
    ///
    /// # 返回
    /// 返回新建的导入行实体列表。
    ///
    /// # 错误
    /// 行字段校验失败时返回错误。
    fn build_rows(
        &self,
        req: &CreateLegacyImportBatchRequest,
        batch_id: &LegacyImportBatchId,
    ) -> Result<Vec<LegacyImportRow>> {
        req.rows
            .iter()
            .map(|row| {
                LegacyImportRow::new(
                    LegacyImportRowId::new(next_id()),
                    LegacyImportRowData {
                        batch_id: batch_id.clone(),
                        source_object_type: row.source_object_type.clone(),
                        source_row_key: row.source_row_key.clone(),
                        normalized_payload_reference: row.normalized_payload_reference.clone(),
                    },
                )
                .map_err(Into::into)
            })
            .collect()
    }

    /// 校验批次存在。
    ///
    /// # 参数
    /// * `batch_id` - 导入批次 ID
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    async fn ensure_batch_exists(&self, batch_id: &str) -> Result<()> {
        self.db
            .legacy_import_batches()
            .find_by_id(batch_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        Ok(())
    }
}

/// 统计指定导入状态的行数。
///
/// # 参数
/// * `rows` - 导入行
/// * `status` - 目标导入状态
///
/// # 返回
/// 返回匹配行数。
fn count_by_status(rows: &[LegacyImportRow], status: ImportStatus) -> u64 {
    rows.iter().filter(|row| row.import_status == status).count() as u64
}

/// 统计仍处于待导入状态的行数。
///
/// # 参数
/// * `rows` - 导入行
///
/// # 返回
/// 返回待导入行数。
fn count_pending(rows: &[LegacyImportRow]) -> u64 {
    count_by_status(rows, ImportStatus::PendingImport)
}
