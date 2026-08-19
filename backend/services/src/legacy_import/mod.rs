//! 域 D22 `legacy_import` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建批次（批次 + 来源行 + D04 后台任务 + 审计日志）→ 跨集合，
//!   `database::Transactional::with_transaction` 内经 `LegacyImportRepository::create_batch_with_rows`
//!   与 D04/D02 仓储写入，保证「批次 + 明细 + 后台任务」原子可见；
//! - 创建确认任务（确认 + `work_item` + 批次摘要 + 审计日志）→ 跨集合事务；
//! - 完成确认（确认决策 + `workflow_action` + 批次 + 任务终态 + 稳定收据）
//!   → `CompleteImportBusinessConfirmation` 唯一强类型事务；
//! - 批次应用（行状态推进 + 批次统计 + 后台任务进度 + 审计日志）→ 跨集合事务；
//! - 其余单集合查询传 `&mut NoTransaction`。
//!
//! 跨域协作只经 `DatabaseExt` 调对方 Repository（P3-service-api §2）：
//! - D04 `bulk_job`：登记/推进 `background_job`（批次导入的后台任务）；
//! - D05 `file_asset`：批次创建时校验资产引用存在；
//! - D07 `party`：客户行导入前校验目标主体存在（`CUSTOMER_NOT_FOUND`）。
//!
//! 幂等约定：批次号、`(batch_id, scope, trial_version)` 与不可逆审计收据
//! 是权威去重依据；重复创建或完成只在全部锁定字段一致时返回原结果。

use database::{
    AccessControlExt, BulkJobExt, DocumentRegistryExt, Executor, FileAssetExt, LegacyImportExt,
    NoTransaction, PartyExt, Transactional, WorkItemExt,
};
use entities::bulk_job::{BackgroundJob, JobStatus};
use entities::common::time::Instant;
use entities::document_registry::{
    BusinessDocumentId, WorkflowAction, WorkflowActionData, WorkflowActionId, WorkflowActionType,
};
use entities::ids::WorkItemId;
use entities::legacy_import::{
    ConfirmationDecision, ConfirmationStatus, ImportStatus, LegacyImportBatch, LegacyImportBatchId,
    LegacyImportBatchStatus, LegacyImportConfirmation, LegacyImportConfirmationData,
    LegacyImportConfirmationId, LegacyImportRow, LegacyImportRowData, LegacyImportRowId, ParseStatus,
};
use entities::work_item::{
    AssignmentSource, WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemStatus,
    WorkItemType,
};
use entities::AuditLog;
use id_generator::next_id;
use mongodb::Database;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::{ProcessingState, WorkItemAllowedAction, WorkItemService, WorkItemView};

pub mod dto;

use self::dto::{
    build_background_job, ApplyRowOutcome, LegacyImportBatchListQuery, SortDir,
    CUSTOMER_NOT_FOUND_ERROR_CODE, CUSTOMER_NOT_FOUND_ERROR_DETAIL, CUSTOMER_OBJECT_TYPE,
};
pub use self::dto::{
    ApplyLegacyImportBatchRequest, ApplyRowResult, CompleteImportBusinessConfirmationCommand,
    CompleteImportBusinessConfirmationResult, CreateLegacyImportBatchRequest,
    CreateLegacyImportConfirmationRequest, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, ImportBusinessConfirmationWorkItemView, ImportExecutionAction,
    ImportExecutionCommand, ImportExecutionNextStep, ImportExecutionResult, ImportExecutionResultStatus,
    ImportRowRequest, LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchView,
    LegacyImportConfirmationListParams, LegacyImportConfirmationView, LegacyImportRowListParams,
    LegacyImportRowView, PageView,
};

const IMPORT_CONFIRMATION_OBJECT_TYPE: &str = "LEGACY_IMPORT_BATCH";
const IMPORT_CONFIRMATION_HANDLER: &str = "import_business_confirmation";
const IMPORT_CONFIRMATION_WORKSPACE: &str = "W18";
const IMPORT_CONFIRMATION_ORGANIZATION: &str = "company";
const IMPORT_CONFIRMATION_AUDIT_PREFIX: &str = "import-confirmation-command-";
const IMPORT_EXECUTION_AUDIT_PREFIX: &str = "import-execution-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

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
        actor: &AuditActor,
        rbac: SharedRbacService,
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
        let work_item_ids = page
            .items
            .iter()
            .map(|row| row.work_item_id.to_string())
            .collect::<Vec<_>>();
        let work_items = self
            .db
            .work_items()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": &work_item_ids } },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let work_item_service = WorkItemService::new(self.db.clone(), rbac);
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let work_item_id = row.work_item_id.to_string();
            let work_item = match work_item_service.work_item_detail(&work_item_id, actor).await {
                Ok(view) => Some(authorized_work_item_view(view, row.status)),
                Err(Error::Forbidden(_) | Error::NotFound(_)) => {
                    work_items.get(&work_item_id).map(read_only_work_item_view)
                }
                Err(error) => return Err(error),
            };
            items.push(LegacyImportConfirmationView {
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
                work_item,
                work_item_id,
                decided_by: row.decided_by,
                decided_at: row.decided_at.map(|at| at as i64),
                version: row.version,
                created_at: row.created_at,
            });
        }

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
    /// 新建开放任务指定当前操作人为个人责任人，责任角色仍按确认范围注册表确定。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人；新建任务以其为个人责任人
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
        let confirmation_scope = registered_confirmation_scope(&req.confirmation_scope)?.to_string();
        let owner_role = confirmation_owner_role(&confirmation_scope)?.to_string();
        let import_rule_version = required_text(&req.import_rule_version, "导入规则版本不能为空")?;
        let subject_version =
            confirmation_subject_version(req.batch_version, req.trial_version, &import_rule_version);
        let confirmation_id = LegacyImportConfirmationId::new(next_id());
        let work_item_id = WorkItemId::new(next_id());
        let confirmation = LegacyImportConfirmation::new(
            confirmation_id,
            LegacyImportConfirmationData {
                batch_id: req.batch_id.clone(),
                confirmation_scope: confirmation_scope.clone(),
                owner_role: owner_role.clone(),
                batch_version: req.batch_version,
                trial_version: req.trial_version,
                import_rule_version: import_rule_version.clone(),
                work_item_id: work_item_id.clone(),
            },
        )?;
        let work_item = import_confirmation_work_item(
            work_item_id,
            &req.batch_id,
            subject_version.clone(),
            &confirmation_scope,
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "legacy_import_confirmation.create",
            "legacy_import_confirmation",
            confirmation.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let confirmation_for_tx = confirmation.clone();
        let work_item_for_tx = work_item.clone();
        let req_for_tx = req.clone();
        let scope_for_tx = confirmation_scope.clone();
        let owner_role_for_tx = owner_role.clone();
        let import_rule_for_tx = import_rule_version.clone();
        let subject_for_tx = subject_version.clone();
        let actor_id = actor.id().to_string();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some(existing) = db
                        .legacy_import_confirmations()
                        .find_by_batch_scope_trial(
                            &req_for_tx.batch_id,
                            &scope_for_tx,
                            req_for_tx.trial_version,
                            session,
                        )
                        .await?
                    {
                        let existing_item = db
                            .work_items()
                            .find_by_id(existing.work_item_id.as_ref(), session)
                            .await?
                            .ok_or_else(|| Error::Internal("导入确认任务关联缺失".to_string()))?;
                        validate_confirmation_creation_replay(
                            &existing,
                            &existing_item,
                            &req_for_tx,
                            &scope_for_tx,
                            &owner_role_for_tx,
                            &import_rule_for_tx,
                            &subject_for_tx,
                        )?;
                        return Ok::<(LegacyImportConfirmation, WorkItem), crate::errors::Error>((
                            existing,
                            existing_item,
                        ));
                    }

                    let mut batch = db
                        .legacy_import_batches()
                        .find_by_id(req_for_tx.batch_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
                    validate_confirmation_creation_batch(
                        &batch,
                        &req_for_tx,
                        &scope_for_tx,
                        &import_rule_for_tx,
                    )?;
                    let enabled_roles = db
                        .roles()
                        .enabled_roles(std::slice::from_ref(&owner_role_for_tx), session)
                        .await?;
                    if enabled_roles.len() != 1 {
                        return Err(Error::BusinessLogicError(
                            "导入确认责任角色未注册或已停用".to_string(),
                        ));
                    }
                    let mut confirmations = db
                        .legacy_import_confirmations()
                        .find_many_sorted(
                            mongodb::bson::doc! { "batch_id": req_for_tx.batch_id.to_string() },
                            mongodb::bson::doc! { "created_at": 1 },
                            session,
                        )
                        .await?;
                    validate_trial_snapshot(&batch, &confirmations, &req_for_tx, &import_rule_for_tx)?;
                    invalidate_replaced_confirmation(
                        &db,
                        &mut confirmations,
                        &confirmation_for_tx,
                        &actor_id,
                        session,
                    )
                    .await?;
                    Self::advance_batch_to_pending_confirmation(&mut batch)?;
                    let mut current_matrix = current_confirmation_matrix(
                        &confirmations,
                        req_for_tx.batch_version,
                        req_for_tx.trial_version,
                        &import_rule_for_tx,
                    );
                    current_matrix.push(confirmation_for_tx.clone());
                    batch.update_summaries(
                        batch.failure_code_summary.clone(),
                        Some(confirmation_matrix_summary(
                            req_for_tx.trial_version,
                            &current_matrix,
                        )),
                    )?;
                    db.legacy_import_confirmations()
                        .create(&confirmation_for_tx, session)
                        .await?;
                    db.work_items().create(&work_item_for_tx, session).await?;
                    db.legacy_import_batches().update(&mut batch, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(LegacyImportConfirmation, WorkItem), crate::errors::Error>((
                        confirmation_for_tx,
                        work_item_for_tx,
                    ))
                })
            })
            .await;
        let (confirmation, work_item) = match transaction_result {
            Ok(result) => result,
            Err(error) => match self
                .replay_confirmation_creation(
                    &req,
                    &confirmation_scope,
                    &owner_role,
                    &import_rule_version,
                    &subject_version,
                )
                .await?
            {
                Some(result) => result,
                None => return Err(error),
            },
        };

        Ok(confirmation_view(confirmation, &work_item))
    }

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
        let fingerprint = confirmation_completion_fingerprint(&prepared);
        let audit_id = confirmation_completion_audit_id(
            actor.id(),
            action,
            prepared.work_item_id.as_ref(),
            &prepared.idempotency_key,
        );
        if let Some(result) = self
            .replay_confirmation_completion(&audit_id, &fingerprint, &prepared)
            .await?
        {
            return Ok(result);
        }
        let decided_at = Instant::now();
        let workflow_action_id = WorkflowActionId::new(next_id());
        let db = self.db.clone();
        let client = db.client().clone();
        let prepared_for_tx = prepared.clone();
        let actor_id = actor.id().to_string();
        let audit_actor = actor.clone();
        let rbac_for_tx = crate::iam::shared_rbac_service(self.db.clone());
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
                    WorkItemService::new(db.clone(), rbac_for_tx.clone())
                        .ensure_domain_decision_access(&audit_actor, &work_item, session)
                        .await?;
                    let _ = &work_item;
                    let mut matrix = db
                        .legacy_import_confirmations()
                        .find_many_sorted(
                            mongodb::bson::doc! { "batch_id": confirmation.batch_id.to_string() },
                            mongodb::bson::doc! { "created_at": 1 },
                            session,
                        )
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
                    let current_matrix = current_confirmation_matrix(
                        &matrix,
                        confirmation.batch_version,
                        confirmation.trial_version,
                        &confirmation.import_rule_version,
                    );
                    let required_scopes = required_confirmation_scopes(&batch.source_object_set)?;
                    let next_step =
                        confirmation_next_step(prepared_for_tx.decision, &current_matrix, &required_scopes);
                    batch.update_summaries(
                        batch.failure_code_summary.clone(),
                        Some(confirmation_matrix_summary(
                            confirmation.trial_version,
                            &current_matrix,
                        )),
                    )?;
                    if next_step == ImportBusinessConfirmationNextStep::StartApply {
                        batch.advance(LegacyImportBatchStatus::ReadyToApply)?;
                    }
                    let workflow_action =
                        confirmation_workflow_action(workflow_action_id, &confirmation, &actor_id)?;
                    db.legacy_import_confirmations()
                        .update(&mut confirmation, session)
                        .await?;
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
                        Some(confirmation_completion_receipt_message(
                            &fingerprint_for_tx,
                            receipt,
                        )),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ConfirmationCompletionTransactionResult, crate::errors::Error>(
                        ConfirmationCompletionTransactionResult {
                            confirmation,
                            work_item,
                            receipt,
                        },
                    )
                })
            })
            .await;
        let result = match transaction_result {
            Ok(result) => result,
            Err(error) => match self
                .replay_confirmation_completion(&audit_id, &fingerprint, &prepared)
                .await?
            {
                Some(result) => return Ok(result),
                None => return Err(error),
            },
        };

        Ok(completion_result(result, audit_id))
    }

    /// 读取并严格核对已创建的同一试算确认任务。
    async fn replay_confirmation_creation(
        &self,
        req: &CreateLegacyImportConfirmationRequest,
        scope: &str,
        owner_role: &str,
        import_rule_version: &str,
        subject_version: &str,
    ) -> Result<Option<(LegacyImportConfirmation, WorkItem)>> {
        let Some(confirmation) = self
            .db
            .legacy_import_confirmations()
            .find_by_batch_scope_trial(&req.batch_id, scope, req.trial_version, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        let work_item = self
            .db
            .work_items()
            .find_by_id(confirmation.work_item_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入确认任务关联缺失".to_string()))?;
        validate_confirmation_creation_replay(
            &confirmation,
            &work_item,
            req,
            scope,
            owner_role,
            import_rule_version,
            subject_version,
        )?;
        Ok(Some((confirmation, work_item)))
    }

    /// 按稳定审计收据重放已提交的导入确认命令。
    async fn replay_confirmation_completion(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        prepared: &PreparedConfirmationCompletion,
    ) -> Result<Option<CompleteImportBusinessConfirmationResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
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
            ConfirmationCompletionTransactionResult {
                confirmation,
                work_item,
                receipt,
            },
            audit_id.to_string(),
        )))
    }

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
        let fingerprint = import_execution_fingerprint(&prepared);
        let audit_id = import_execution_audit_id(
            actor.id(),
            action_name,
            prepared.batch_id.as_ref(),
            &prepared.request_id,
        );
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
            .await?
            .ok_or_else(|| Error::Internal("导入批次后台任务缺失".to_string()))?;
        if !matches!(
            background_job.status,
            JobStatus::Running | JobStatus::PartiallySucceeded
        ) {
            return Err(Error::BusinessLogicError(
                "后台应用尚未由 START_APPLY 启动".to_string(),
            ));
        }

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
                    Self::advance_background_job(
                        &mut job_for_tx,
                        delta_success,
                        delta_skipped,
                        delta_failed,
                        all_terminal,
                        now,
                    )?;
                    db.background_jobs().update(&mut job_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<LegacyImportBatch, crate::errors::Error>(batch_for_tx)
                })
            })
            .await?;

        self.batch_view_of(updated_batch).await
    }

    /// 推进后台任务进度并收尾。
    ///
    /// 任务必须已由 `START_APPLY` 启动；按本批结果累加进度；全部行终态时
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
        if !matches!(job.status, JobStatus::Running | JobStatus::PartiallySucceeded) {
            return Err(Error::BusinessLogicError(
                "后台任务未启动，禁止记录导入进度".to_string(),
            ));
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedImportExecution {
    batch_id: LegacyImportBatchId,
    expected_batch_version: u64,
    expected_trial_version: Option<u32>,
    action: ImportExecutionAction,
    reason_code: Option<String>,
    comment: Option<String>,
    request_id: String,
}

impl TryFrom<ImportExecutionCommand> for PreparedImportExecution {
    type Error = Error;

    fn try_from(command: ImportExecutionCommand) -> Result<Self> {
        let expected_trial_version = command
            .expected_trial_version
            .as_deref()
            .map(|value| parse_command_version(value, "试算版本"))
            .transpose()?;
        if matches!(
            command.action,
            ImportExecutionAction::StartApply | ImportExecutionAction::RetryFailed
        ) && expected_trial_version.is_none()
        {
            return Err(Error::ValidationError(
                "提交应用或重试失败项必须携带试算版本".to_string(),
            ));
        }
        let reason_code = optional_text(command.reason_code);
        if command.action == ImportExecutionAction::CancelPending && reason_code.is_none() {
            return Err(Error::ValidationError("取消尚未应用项必须提供原因码".to_string()));
        }
        if command.action == ImportExecutionAction::StartApply && reason_code.is_some() {
            return Err(Error::ValidationError(
                "提交应用不得携带取消或重试原因码".to_string(),
            ));
        }
        Ok(Self {
            batch_id: command.batch_id,
            expected_batch_version: parse_command_version(&command.expected_batch_version, "批次版本")?,
            expected_trial_version,
            action: command.action,
            reason_code,
            comment: optional_text(command.comment),
            request_id: required_text(&command.request_id, "请求身份不能为空")?,
        })
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
    retry_row_ids: BTreeSet<String>,
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
    if batch.base.version != prepared.expected_batch_version {
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
        .find_many_sorted(
            mongodb::bson::doc! { "batch_id": batch.base.id.clone() },
            mongodb::bson::doc! { "created_at": 1 },
            executor,
        )
        .await?;
    let trial_version = validate_import_execution_trial(prepared, &batch, &confirmations)?;
    let mut rows = if prepared.action == ImportExecutionAction::RetryFailed {
        db.legacy_import_rows()
            .find_rows_by_batch_ids(std::slice::from_ref(&prepared.batch_id), executor)
            .await?
    } else {
        Vec::new()
    };
    let outcome = apply_import_execution_action(prepared, &mut batch, &mut job, &mut rows)?;
    for row in rows
        .iter_mut()
        .filter(|row| outcome.retry_row_ids.contains(&row.base.id))
    {
        db.legacy_import_rows().update(row, executor).await?;
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
        && job.domain_job_type.as_deref() == Some(dto::LEGACY_IMPORT_DOMAIN_JOB_TYPE)
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
    let current_trial = confirmations
        .iter()
        .filter(|item| {
            item.status != ConfirmationStatus::Invalidated
                && item.import_rule_version == batch.import_rule_version
        })
        .map(|item| item.trial_version)
        .max()
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
    let required = required_confirmation_scopes(&batch.source_object_set)?;
    let current = confirmations
        .iter()
        .filter(|item| {
            item.trial_version == trial_version
                && item.import_rule_version == batch.import_rule_version
                && item.status != ConfirmationStatus::Invalidated
        })
        .collect::<Vec<_>>();
    let confirmed = current
        .iter()
        .filter(|item| item.status == ConfirmationStatus::Confirmed)
        .map(|item| item.confirmation_scope.as_str())
        .collect::<BTreeSet<_>>();
    if current
        .iter()
        .any(|item| item.status == ConfirmationStatus::Rejected)
        || !required.iter().all(|scope| confirmed.contains(scope))
    {
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
    if batch.status != LegacyImportBatchStatus::ReadyToApply || job.status != JobStatus::Pending {
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
        retry_row_ids: BTreeSet::new(),
    })
}

/// 取消尚未应用项，已处理计数和行事实原样保留。
fn cancel_pending_import(
    batch: &mut LegacyImportBatch,
    job: &mut BackgroundJob,
) -> Result<ImportExecutionActionOutcome> {
    if !matches!(
        batch.status,
        LegacyImportBatchStatus::ReadyToApply | LegacyImportBatchStatus::Importing
    ) || !matches!(
        job.status,
        JobStatus::Pending | JobStatus::Running | JobStatus::PartiallySucceeded
    ) {
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
        retry_row_ids: BTreeSet::new(),
    })
}

/// 仅重新准备失败行，保留已导入、已跳过与未处理行。
fn prepare_failed_import_retry(
    batch: &mut LegacyImportBatch,
    job: &mut BackgroundJob,
    rows: &mut [LegacyImportRow],
) -> Result<ImportExecutionActionOutcome> {
    if !matches!(
        batch.status,
        LegacyImportBatchStatus::PartialFailed | LegacyImportBatchStatus::Failed
    ) {
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
    for row in rows.iter_mut().filter(|row| retry_row_ids.contains(&row.base.id)) {
        row.prepare_failed_retry()?;
    }
    let affected_items = retry_row_ids.len() as u64;
    job.prepare_failed_retry(affected_items, Instant::now())?;
    batch.update_counts(
        batch.total_rows,
        count_by_status(rows, ImportStatus::Imported),
        count_by_status(rows, ImportStatus::Failed),
    )?;
    batch.advance(LegacyImportBatchStatus::ReadyToApply)?;
    Ok(ImportExecutionActionOutcome {
        result_status: ImportExecutionResultStatus::RetryPrepared,
        next_step: ImportExecutionNextStep::StartApply,
        affected_items,
        retry_row_ids,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedConfirmationCompletion {
    work_item_id: WorkItemId,
    batch_id: LegacyImportBatchId,
    expected_task_version: u64,
    expected_subject_version: String,
    expected_batch_version: u64,
    expected_trial_version: u32,
    confirmation_scope: String,
    decision: ConfirmationDecision,
    reason_code: Option<String>,
    comment: Option<String>,
    idempotency_key: String,
}

impl TryFrom<CompleteImportBusinessConfirmationCommand> for PreparedConfirmationCompletion {
    type Error = Error;

    fn try_from(command: CompleteImportBusinessConfirmationCommand) -> Result<Self> {
        let decision = command.decision;
        let confirmation_scope = registered_confirmation_scope(&decision.confirmation_scope)?.to_string();
        let reason_code = optional_text(decision.reason_code);
        if decision.action == ConfirmationDecision::ReturnForFix && reason_code.is_none() {
            return Err(Error::ValidationError("退回修复必须提供原因代码".to_string()));
        }
        if decision.action == ConfirmationDecision::ConfirmScope && reason_code.is_some() {
            return Err(Error::ValidationError("确认责任范围不得携带退回原因".to_string()));
        }
        Ok(Self {
            work_item_id: command.work_item_id,
            batch_id: decision.batch_id,
            expected_task_version: parse_command_version(&command.expected_task_version, "任务版本")?,
            expected_subject_version: required_text(
                &command.expected_subject_version,
                "任务主体版本不能为空",
            )?,
            expected_batch_version: parse_command_version(&decision.expected_batch_version, "批次版本")?,
            expected_trial_version: parse_command_version(&decision.expected_trial_version, "试算版本")?,
            confirmation_scope,
            decision: decision.action,
            reason_code,
            comment: optional_text(decision.comment),
            idempotency_key: required_text(&command.idempotency_key, "幂等键不能为空")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConfirmationCompletionReceipt {
    result_status: ImportBusinessConfirmationResultStatus,
    task_version: u64,
    batch_version: u64,
    next_step: ImportBusinessConfirmationNextStep,
}

struct ConfirmationCompletionTransactionResult {
    confirmation: LegacyImportConfirmation,
    work_item: WorkItem,
    receipt: ConfirmationCompletionReceipt,
}

/// 返回经编译期固定的确认范围代码。
fn registered_confirmation_scope(scope: &str) -> Result<&'static str> {
    match scope.trim().to_ascii_uppercase().as_str() {
        "SALES" => Ok("SALES"),
        "PROCUREMENT" => Ok("PROCUREMENT"),
        "OPERATIONS" => Ok("OPERATIONS"),
        "WAREHOUSE" => Ok("WAREHOUSE"),
        "FINANCE" => Ok("FINANCE"),
        _ => Err(Error::ValidationError(
            "确认范围未在 W18 固定注册表中".to_string(),
        )),
    }
}

/// 按 W18 固定注册表解析责任角色。
fn confirmation_owner_role(scope: &str) -> Result<&'static str> {
    match registered_confirmation_scope(scope)? {
        "SALES" => Ok("role-sales"),
        "PROCUREMENT" => Ok("role-procurement"),
        "OPERATIONS" => Ok("role-operations"),
        "WAREHOUSE" => Ok("role-warehouse"),
        "FINANCE" => Ok("role-finance"),
        _ => unreachable!("已经过固定范围注册表校验"),
    }
}

/// 构造采用固定责任范围维度的 W18 正常导入确认任务。
///
/// 开放任务必须在创建时指定唯一个人责任人，责任角色仍由已注册
/// `confirmation_scope` 决定。
///
/// # 参数
/// * `work_item_id` - 任务主键
/// * `batch_id` - 导入批次
/// * `subject_version` - 确认任务对应的试算版本
/// * `confirmation_scope` - 已注册确认范围
/// * `owner_user_id` - 当前个人责任人
///
/// # 返回
/// 返回带冻结 `responsibility_key` 的开放任务。
///
/// # 错误
/// 确认范围未注册、责任角色无法解析，或任务字段校验失败时返回错误。
fn import_confirmation_work_item(
    work_item_id: WorkItemId,
    batch_id: &LegacyImportBatchId,
    subject_version: String,
    confirmation_scope: &str,
    owner_user_id: &str,
) -> Result<WorkItem> {
    let confirmation_scope = registered_confirmation_scope(confirmation_scope)?;
    let owner_role = confirmation_owner_role(confirmation_scope)?;
    Ok(WorkItem::new_with_responsibility_key(
        work_item_id,
        WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: IMPORT_CONFIRMATION_OBJECT_TYPE.to_string(),
            business_object_id: batch_id.to_string(),
            subject_version,
            owner_role: owner_role.to_string(),
            owner_organization_id: IMPORT_CONFIRMATION_ORGANIZATION.to_string(),
            owner_user_id: Some(owner_user_id.to_string()),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("IMPORT_TRIAL_CONFIRMATION".to_string()),
            impact_summary: Some(format!("{confirmation_scope}范围导入试算待业务确认")),
        },
        confirmation_scope,
    )?)
}

/// 从批次对象集解析当前试算的必要责任范围。
fn required_confirmation_scopes(source_object_set: &str) -> Result<BTreeSet<&'static str>> {
    let mut scopes = BTreeSet::new();
    for raw in source_object_set.split([',', ';', '|', '/', '、']) {
        let object = raw.trim().to_ascii_uppercase();
        if object.is_empty() {
            continue;
        }
        match object.as_str() {
            "CUSTOMER" | "CONTRACT" | "CARD_SALES_ORDER" | "客户" | "合同" | "卡券销售" | "卡券销售单" =>
            {
                scopes.insert("SALES");
            }
            "SUPPLIER" | "SKU" | "供应商" | "商品SKU" | "商品 SKU" => {
                scopes.insert("PROCUREMENT");
            }
            "CARD_CATEGORY" | "卡券类目" => {
                scopes.insert("OPERATIONS");
            }
            "WAREHOUSE" | "OPENING_STOCK" | "仓库" | "期初库存" => {
                scopes.insert("WAREHOUSE");
            }
            "CARD_OPENING_AR" | "卡券期初应收" | "期初应收" => {
                scopes.insert("FINANCE");
            }
            _ => {
                return Err(Error::BusinessLogicError(format!(
                    "批次对象类型 {raw} 未配置 W18 确认责任"
                )));
            }
        }
    }
    if scopes.is_empty() {
        return Err(Error::BusinessLogicError(
            "批次对象集无法解析必要确认责任".to_string(),
        ));
    }
    Ok(scopes)
}

/// 生成任务冻结的试算主体版本。
fn confirmation_subject_version(batch_version: u32, trial_version: u32, rule_version: &str) -> String {
    format!("batch:{batch_version};trial:{trial_version};rule:{rule_version}")
}

/// 校验创建确认任务时的批次与责任范围。
fn validate_confirmation_creation_batch(
    batch: &LegacyImportBatch,
    req: &CreateLegacyImportConfirmationRequest,
    scope: &str,
    import_rule_version: &str,
) -> Result<()> {
    if !matches!(
        batch.status,
        LegacyImportBatchStatus::PendingValidation
            | LegacyImportBatchStatus::Validating
            | LegacyImportBatchStatus::PendingConfirmation
    ) {
        return Err(Error::BusinessLogicError(
            "批次已离开可确认阶段，禁止创建确认任务".to_string(),
        ));
    }
    if batch.import_rule_version != import_rule_version {
        return Err(Error::ConflictError("导入规则版本已变化，请重新试算".to_string()));
    }
    if !required_confirmation_scopes(&batch.source_object_set)?.contains(scope) {
        return Err(Error::BusinessLogicError(
            "该责任范围不属于当前批次的必要确认矩阵".to_string(),
        ));
    }
    if req.batch_version == u32::MAX && req.trial_version == u32::MAX {
        return Err(Error::ValidationError(
            "批次与试算版本不能同时到达上限".to_string(),
        ));
    }
    Ok(())
}

/// 校验同一试算矩阵的版本一致性和单调性。
fn validate_trial_snapshot(
    batch: &LegacyImportBatch,
    confirmations: &[LegacyImportConfirmation],
    req: &CreateLegacyImportConfirmationRequest,
    import_rule_version: &str,
) -> Result<()> {
    let max_trial = confirmations.iter().map(|item| item.trial_version).max();
    if max_trial.is_some_and(|trial| trial > req.trial_version) {
        return Err(Error::ConflictError("新试算版本不得低于已有确认版本".to_string()));
    }
    let same_trial = confirmations
        .iter()
        .filter(|item| item.trial_version == req.trial_version)
        .collect::<Vec<_>>();
    if same_trial.iter().any(|item| {
        item.batch_version != req.batch_version || item.import_rule_version != import_rule_version
    }) {
        return Err(Error::ConflictError(
            "同一试算矩阵的批次或规则版本不一致".to_string(),
        ));
    }
    if same_trial
        .iter()
        .any(|item| item.status == ConfirmationStatus::Rejected)
    {
        return Err(Error::ConflictError(
            "当前试算已被退回，必须修复并生成新试算版本".to_string(),
        ));
    }
    if batch.import_rule_version != import_rule_version {
        return Err(Error::ConflictError("导入规则版本已变化".to_string()));
    }
    Ok(())
}

/// 严格校验重复创建是否与已有事实及任务完全一致。
fn validate_confirmation_creation_replay(
    confirmation: &LegacyImportConfirmation,
    work_item: &WorkItem,
    req: &CreateLegacyImportConfirmationRequest,
    scope: &str,
    owner_role: &str,
    import_rule_version: &str,
    subject_version: &str,
) -> Result<()> {
    let exact = confirmation.batch_id == req.batch_id
        && confirmation.confirmation_scope == scope
        && confirmation.owner_role == owner_role
        && confirmation.batch_version == req.batch_version
        && confirmation.trial_version == req.trial_version
        && confirmation.import_rule_version == import_rule_version
        && work_item.base.id == confirmation.work_item_id.to_string()
        && work_item.work_item_type == WorkItemType::ImportBusinessConfirmation
        && work_item.business_object_type == IMPORT_CONFIRMATION_OBJECT_TYPE
        && work_item.business_object_id == req.batch_id.to_string()
        && work_item.responsibility_key() == Some(scope)
        && work_item.subject_version == subject_version
        && work_item.owner_role == owner_role
        && work_item.owner_organization_id == IMPORT_CONFIRMATION_ORGANIZATION;
    if exact {
        return Ok(());
    }
    Err(Error::ConflictError(
        "同一批次、范围与试算版本已用于不同的确认任务".to_string(),
    ))
}

/// 将新试算取代的旧待确认事实失效，并关闭关联任务。
async fn invalidate_replaced_confirmation(
    db: &Database,
    confirmations: &mut [LegacyImportConfirmation],
    replacement: &LegacyImportConfirmation,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    for confirmation in confirmations.iter_mut().filter(|item| {
        item.status == ConfirmationStatus::Pending && item.trial_version < replacement.trial_version
    }) {
        confirmation.invalidate(
            LegacyImportConfirmationId::new(replacement.base.id.clone()),
            Instant::now(),
        )?;
        db.legacy_import_confirmations()
            .update(confirmation, executor)
            .await?;
        let mut work_item = db
            .work_items()
            .find_by_id(confirmation.work_item_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::Internal("被新试算取代的确认任务缺失".to_string()))?;
        if work_item.status == WorkItemStatus::Open {
            work_item.close(
                actor_id,
                WorkItemCloseData {
                    close_reason: "SUPERSEDED_BY_NEW_IMPORT_TRIAL".to_string(),
                },
                Instant::now(),
            )?;
            db.work_items().update(&mut work_item, executor).await?;
        }
    }
    Ok(())
}

/// 返回当前试算快照中的确认矩阵。
fn current_confirmation_matrix(
    confirmations: &[LegacyImportConfirmation],
    batch_version: u32,
    trial_version: u32,
    import_rule_version: &str,
) -> Vec<LegacyImportConfirmation> {
    confirmations
        .iter()
        .filter(|item| {
            item.batch_version == batch_version
                && item.trial_version == trial_version
                && item.import_rule_version == import_rule_version
                && item.status != ConfirmationStatus::Invalidated
        })
        .cloned()
        .collect()
}

/// 将本次决策后的事实替换进内存矩阵。
fn replace_confirmation_in_matrix(
    confirmations: &mut [LegacyImportConfirmation],
    decided: &LegacyImportConfirmation,
) {
    if let Some(current) = confirmations
        .iter_mut()
        .find(|item| item.base.id == decided.base.id)
    {
        *current = decided.clone();
    }
}

/// 计算强类型决策后的唯一下一步。
fn confirmation_next_step(
    decision: ConfirmationDecision,
    confirmations: &[LegacyImportConfirmation],
    required_scopes: &BTreeSet<&'static str>,
) -> ImportBusinessConfirmationNextStep {
    if decision == ConfirmationDecision::ReturnForFix {
        return ImportBusinessConfirmationNextStep::FixAndRevalidate;
    }
    let confirmed_scopes = confirmations
        .iter()
        .filter(|item| item.status == ConfirmationStatus::Confirmed)
        .map(|item| item.confirmation_scope.as_str())
        .collect::<BTreeSet<_>>();
    if required_scopes
        .iter()
        .all(|scope| confirmed_scopes.contains(scope))
    {
        ImportBusinessConfirmationNextStep::StartApply
    } else {
        ImportBusinessConfirmationNextStep::AwaitOtherConfirmations
    }
}

/// 生成不含个人身份的批次确认派生摘要。
fn confirmation_matrix_summary(trial_version: u32, confirmations: &[LegacyImportConfirmation]) -> String {
    let states = confirmations
        .iter()
        .map(|item| (item.confirmation_scope.as_str(), item.status.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let encoded = states
        .into_iter()
        .map(|(scope, status)| format!("{scope}={status}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("trial={trial_version};{encoded}")
}

/// 校验完成命令锁定的任务、事实、批次和当前责任。
fn validate_confirmation_completion(
    command: &PreparedConfirmationCompletion,
    work_item: &WorkItem,
    confirmation: &LegacyImportConfirmation,
    batch: &LegacyImportBatch,
    actor_id: &str,
) -> Result<()> {
    if work_item.base.version != command.expected_task_version
        || batch.base.version != command.expected_batch_version
    {
        return Err(Error::ConflictError(
            "导入确认任务或批次版本已变化，请刷新后重试".to_string(),
        ));
    }
    let expected_subject = confirmation_subject_version(
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
    let owner_role = confirmation_owner_role(&command.confirmation_scope)?;
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
        && confirmation.status == ConfirmationStatus::Pending
        && batch.base.id == confirmation.batch_id.to_string()
        && batch.status == LegacyImportBatchStatus::PendingConfirmation
        && batch.import_rule_version == confirmation.import_rule_version;
    if !task_matches || !fact_matches {
        return Err(Error::BusinessLogicError(
            "导入确认任务、责任范围或批次不匹配".to_string(),
        ));
    }
    if !required_confirmation_scopes(&batch.source_object_set)?.contains(command.confirmation_scope.as_str())
    {
        return Err(Error::BusinessLogicError(
            "当前确认范围已不属于批次必要矩阵".to_string(),
        ));
    }
    if !work_item.is_owned_by(actor_id) {
        return Err(Error::Forbidden("当前账号不是该导入确认的当前责任人".to_string()));
    }
    Ok(())
}

/// 构造确认事实对应的追加式 `workflow_action`。
fn confirmation_workflow_action(
    id: WorkflowActionId,
    confirmation: &LegacyImportConfirmation,
    actor_id: &str,
) -> Result<WorkflowAction> {
    let (action_type, to_status, comment) = match confirmation.decision {
        Some(ConfirmationDecision::ConfirmScope) => (WorkflowActionType::Confirm, "CONFIRMED", None),
        Some(ConfirmationDecision::ReturnForFix) => (
            WorkflowActionType::Reject,
            "REJECTED",
            confirmation.reason_code.clone(),
        ),
        None => return Err(Error::Internal("导入确认动作缺少领域决策".to_string())),
    };
    WorkflowAction::new(
        id,
        WorkflowActionData {
            document_id: BusinessDocumentId::new(confirmation.batch_id.to_string()),
            action_type,
            from_status: "PENDING".to_string(),
            to_status: to_status.to_string(),
            actor_id: actor_id.to_string(),
            actor_role: confirmation.owner_role.clone(),
            comment,
        },
    )
    .map_err(Into::into)
}

/// 把任务实体映射为 W18 真实任务投影。
fn work_item_view(item: &WorkItem) -> ImportBusinessConfirmationWorkItemView {
    ImportBusinessConfirmationWorkItemView {
        work_item_id: item.base.id.clone(),
        work_item_type: item.work_item_type,
        task_version: item.base.version.to_string(),
        subject_version: item.subject_version.clone(),
        status: item.status,
        assignment_source_unused: item.assignment_source,
        owner_role: item.owner_role.clone(),
        owner_organization_id: item.owner_organization_id.clone(),
        owner_user_id: item.owner_user_id.clone(),
        processing_state: "READY".to_string(),
        allowed_actions: Vec::new(),
        action_blockers: Vec::new(),
        handler_key: IMPORT_CONFIRMATION_HANDLER.to_string(),
        destination_workspace_id: IMPORT_CONFIRMATION_WORKSPACE.to_string(),
    }
}

/// 为不在当前责任范围的查询人返回最小只读任务投影。
fn read_only_work_item_view(item: &WorkItem) -> ImportBusinessConfirmationWorkItemView {
    let mut view = work_item_view(item);
    view.owner_user_id = None;
    if item.status == WorkItemStatus::Open {
        view.action_blockers
            .push("当前账号不在该责任范围，任务仅可查看。".to_string());
    }
    view
}

/// 把统一待办的 actor 安全投影合并为 W18 责任与领域动作。
fn authorized_work_item_view(
    item: WorkItemView,
    confirmation_status: ConfirmationStatus,
) -> ImportBusinessConfirmationWorkItemView {
    let mut allowed_actions = item
        .allowed_actions
        .iter()
        .copied()
        .map(work_item_action_code)
        .map(str::to_string)
        .collect::<Vec<_>>();
    append_confirmation_actions(&mut allowed_actions, confirmation_status, &item.allowed_actions);
    let mut action_blockers = item
        .action_blockers
        .into_iter()
        .map(|blocker| blocker.message)
        .collect::<Vec<_>>();
    if let Some(blocker) = item.processing_blocker {
        action_blockers.push(blocker.message);
    }
    ImportBusinessConfirmationWorkItemView {
        work_item_id: item.id,
        work_item_type: item.work_item_type,
        task_version: item.task_version,
        subject_version: item.subject_version,
        status: item.status,
        assignment_source_unused: item.assignment_source,
        owner_role: item.owner_role,
        owner_organization_id: item.owner_organization_id,
        owner_user_id: item.owner_user_id,
        processing_state: processing_state_code(item.processing_state).to_string(),
        allowed_actions,
        action_blockers,
        handler_key: item.handler_key,
        destination_workspace_id: item.destination_workspace_id,
    }
}

/// 只有当前责任人且确认事实仍待处理时，才追加 W18 正式领域动作。
fn append_confirmation_actions(
    actions: &mut Vec<String>,
    confirmation_status: ConfirmationStatus,
    responsibility_actions: &[WorkItemAllowedAction],
) {
    if confirmation_status != ConfirmationStatus::Pending
        || !responsibility_actions.contains(&WorkItemAllowedAction::Process)
    {
        return;
    }
    actions.push("CONFIRM_SCOPE".to_string());
    actions.push("RETURN_FOR_FIX".to_string());
}

/// 返回统一责任动作的稳定 wire code。
fn work_item_action_code(action: WorkItemAllowedAction) -> &'static str {
    match action {
        WorkItemAllowedAction::View => "VIEW",
        WorkItemAllowedAction::Process => "PROCESS",
        WorkItemAllowedAction::Reassign => "RELEASE_TO_TEAM",
        WorkItemAllowedAction::Close => "CLOSE",
    }
}

/// 返回统一处理状态的稳定 wire code。
fn processing_state_code(state: ProcessingState) -> &'static str {
    match state {
        ProcessingState::Ready => "READY",
        ProcessingState::ApprovalBlocked => "APPROVAL_BLOCKED",
    }
}

/// 合并确认事实与对应任务投影。
fn confirmation_view(
    confirmation: LegacyImportConfirmation,
    work_item: &WorkItem,
) -> LegacyImportConfirmationView {
    let mut view: LegacyImportConfirmationView = confirmation.into();
    view.work_item = Some(work_item_view(work_item));
    view
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
fn confirmation_result_status(decision: ConfirmationDecision) -> ImportBusinessConfirmationResultStatus {
    match decision {
        ConfirmationDecision::ConfirmScope => ImportBusinessConfirmationResultStatus::Confirmed,
        ConfirmationDecision::ReturnForFix => ImportBusinessConfirmationResultStatus::Rejected,
    }
}

/// 生成不暴露原始幂等键的稳定审计主键。
fn confirmation_completion_audit_id(
    actor_id: &str,
    action: &str,
    work_item_id: &str,
    idempotency_key: &str,
) -> String {
    format!(
        "{IMPORT_CONFIRMATION_AUDIT_PREFIX}{}",
        stable_digest(&format!("{actor_id}|{action}|{work_item_id}|{idempotency_key}"))
    )
}

/// 对命令全部版本锁与业务载荷生成无歧义指纹。
fn confirmation_completion_fingerprint(command: &PreparedConfirmationCompletion) -> String {
    command_fingerprint(&[
        command.work_item_id.as_ref(),
        command.batch_id.as_ref(),
        &command.expected_task_version.to_string(),
        &command.expected_subject_version,
        &command.expected_batch_version.to_string(),
        &command.expected_trial_version.to_string(),
        &command.confirmation_scope,
        command.decision.as_str(),
        command.reason_code.as_deref().unwrap_or_default(),
        command.comment.as_deref().unwrap_or_default(),
    ])
}

/// 将导入确认的最小结果收据编码到审计消息。
fn confirmation_completion_receipt_message(
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
fn parse_confirmation_completion_receipt(
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

/// 生成不暴露原始 `request_id` 的导入执行收据主键。
fn import_execution_audit_id(actor_id: &str, action: &str, batch_id: &str, request_id: &str) -> String {
    format!(
        "{IMPORT_EXECUTION_AUDIT_PREFIX}{}",
        stable_digest(&format!("{actor_id}|{action}|{batch_id}|{request_id}"))
    )
}

/// 对导入执行命令全部版本锁与载荷生成无歧义指纹。
fn import_execution_fingerprint(command: &PreparedImportExecution) -> String {
    command_fingerprint(&[
        command.batch_id.as_ref(),
        &command.expected_batch_version.to_string(),
        &command
            .expected_trial_version
            .map(|value| value.to_string())
            .unwrap_or_default(),
        command.action.as_str(),
        command.reason_code.as_deref().unwrap_or_default(),
        command.comment.as_deref().unwrap_or_default(),
    ])
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

/// 对各字段分别加长度前缀后计算命令摘要。
fn command_fingerprint(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

/// 计算稳定 SHA-256 十六进制摘要。
fn stable_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// 解析收据中的整数版本。
fn parse_receipt_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| Error::Internal(format!("导入确认幂等收据{field}非法")))
}

/// 归一化必填文本。
fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 把 HTTP 边界的字符串版本严格解析为正整数。
fn parse_command_version<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr + PartialEq + From<u8>,
{
    let value = required_text(value, &format!("{field}不能为空"))?;
    let parsed = value
        .parse::<T>()
        .map_err(|_| Error::ValidationError(format!("{field}必须是正整数")))?;
    if parsed == T::from(0) {
        return Err(Error::ValidationError(format!("{field}必须是正整数")));
    }
    Ok(parsed)
}

/// 归一化可选文本，空白值折叠为空。
fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
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

#[cfg(test)]
mod tests {
    use entities::common::time::BusinessDate;
    use entities::ids::{
        ExternalIdentityMapId, LegacyImportBatchId, LegacyImportConfirmationId, LegacyImportRowId,
        SourceSystemId, WorkItemId,
    };
    use entities::legacy_import::{LegacyImportBatchData, LegacyImportConfirmationData, LegacyImportRowData};

    use super::*;

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

    fn confirmation() -> LegacyImportConfirmation {
        LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("confirmation-1"),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: "SALES".to_string(),
                owner_role: "role-sales".to_string(),
                batch_version: 1,
                trial_version: 2,
                import_rule_version: "rule-1".to_string(),
                work_item_id: WorkItemId::new("work-item-1"),
            },
        )
        .unwrap()
    }

    fn work_item() -> WorkItem {
        let mut item = import_confirmation_work_item(
            WorkItemId::new("work-item-1"),
            &LegacyImportBatchId::new("batch-1"),
            confirmation_subject_version(1, 2, "rule-1"),
            "SALES",
            "user-1",
        )
        .unwrap();
        item.base.version = 3;
        item
    }

    fn completion_command() -> PreparedConfirmationCompletion {
        PreparedConfirmationCompletion {
            work_item_id: WorkItemId::new("work-item-1"),
            batch_id: LegacyImportBatchId::new("batch-1"),
            expected_task_version: 3,
            expected_subject_version: confirmation_subject_version(1, 2, "rule-1"),
            expected_batch_version: 4,
            expected_trial_version: 2,
            confirmation_scope: "SALES".to_string(),
            decision: ConfirmationDecision::ConfirmScope,
            reason_code: None,
            comment: Some("确认".to_string()),
            idempotency_key: "request-1".to_string(),
        }
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
    fn scope_registry_fixes_role_and_rejects_unknown_values() {
        assert_eq!(confirmation_owner_role("sales").unwrap(), "role-sales");
        assert_eq!(confirmation_owner_role("FINANCE").unwrap(), "role-finance");
        assert!(confirmation_owner_role("SYSADMIN").is_err());
    }

    #[test]
    fn object_set_derives_only_registered_required_scopes() {
        let scopes =
            required_confirmation_scopes("CUSTOMER;SUPPLIER|CARD_CATEGORY/期初库存、CARD_OPENING_AR")
                .unwrap();

        assert_eq!(
            scopes,
            BTreeSet::from(["FINANCE", "OPERATIONS", "PROCUREMENT", "SALES", "WAREHOUSE"])
        );
        assert!(required_confirmation_scopes("UNREGISTERED_OBJECT").is_err());
    }

    #[test]
    fn create_command_rejects_client_owned_task_fields() {
        let payload = serde_json::json!({
            "batch_id": "batch-1",
            "confirmation_scope": "SALES",
            "owner_role": "role-root",
            "batch_version": 1,
            "trial_version": 2,
            "import_rule_version": "rule-1",
            "work_item_id": "forged-task"
        });

        assert!(serde_json::from_value::<CreateLegacyImportConfirmationRequest>(payload).is_err());
    }

    #[test]
    fn same_batch_confirmation_scopes_use_distinct_server_responsibility_keys() {
        let batch_id = LegacyImportBatchId::new("batch-1");
        let subject_version = confirmation_subject_version(1, 2, "rule-1");
        let sales = import_confirmation_work_item(
            WorkItemId::new("work-item-sales"),
            &batch_id,
            subject_version.clone(),
            " sales ",
            "user-sales",
        )
        .unwrap();
        let procurement = import_confirmation_work_item(
            WorkItemId::new("work-item-procurement"),
            &batch_id,
            subject_version,
            "PROCUREMENT",
            "user-procurement",
        )
        .unwrap();

        assert_eq!(sales.business_object_id, "batch-1");
        assert_eq!(procurement.business_object_id, "batch-1");
        assert_eq!(sales.responsibility_key(), Some("SALES"));
        assert_eq!(procurement.responsibility_key(), Some("PROCUREMENT"));
        assert_eq!(sales.owner_role, "role-sales");
        assert_eq!(procurement.owner_role, "role-procurement");
    }

    #[test]
    fn completion_requires_exact_task_subject_batch_and_current_owner() {
        let command = completion_command();
        let item = work_item();
        let confirmation = confirmation();
        let batch = batch();

        validate_confirmation_completion(&command, &item, &confirmation, &batch, "user-1").unwrap();
        assert!(
            validate_confirmation_completion(&command, &item, &confirmation, &batch, "other-user").is_err()
        );
        let mut stale = command;
        stale.expected_trial_version = 3;
        assert!(validate_confirmation_completion(&stale, &item, &confirmation, &batch, "user-1").is_err());
    }

    #[test]
    fn domain_actions_require_pending_fact_and_process_responsibility() {
        let mut mine = vec!["VIEW".to_string(), "PROCESS".to_string()];
        append_confirmation_actions(
            &mut mine,
            ConfirmationStatus::Pending,
            &[WorkItemAllowedAction::View, WorkItemAllowedAction::Process],
        );
        assert_eq!(mine, ["VIEW", "PROCESS", "CONFIRM_SCOPE", "RETURN_FOR_FIX"]);

        let mut view_only = vec!["VIEW".to_string()];
        append_confirmation_actions(
            &mut view_only,
            ConfirmationStatus::Pending,
            &[WorkItemAllowedAction::View],
        );
        assert_eq!(view_only, ["VIEW"]);

        let mut completed = vec!["VIEW".to_string(), "PROCESS".to_string()];
        append_confirmation_actions(
            &mut completed,
            ConfirmationStatus::Confirmed,
            &[WorkItemAllowedAction::View, WorkItemAllowedAction::Process],
        );
        assert_eq!(completed, ["VIEW", "PROCESS"]);
    }

    #[test]
    fn unauthorized_projection_masks_current_owner_and_has_no_actions() {
        let view = read_only_work_item_view(&work_item());

        assert_eq!(view.owner_user_id, None);
        assert!(view.allowed_actions.is_empty());
        assert_eq!(view.action_blockers, ["当前账号不在该责任范围，任务仅可查看。"]);
    }

    #[test]
    fn return_for_fix_is_rejected_without_successor() {
        let next = confirmation_next_step(
            ConfirmationDecision::ReturnForFix,
            &[confirmation()],
            &BTreeSet::from(["SALES"]),
        );

        assert_eq!(next, ImportBusinessConfirmationNextStep::FixAndRevalidate);
        assert_eq!(
            confirmation_result_status(ConfirmationDecision::ReturnForFix),
            ImportBusinessConfirmationResultStatus::Rejected
        );
    }

    #[test]
    fn last_confirmation_prepares_batch_without_starting_application() {
        let mut sales = confirmation();
        sales
            .decide(
                ConfirmationDecision::ConfirmScope,
                "sales-user".to_string(),
                Instant::from_unix_secs(1_700_000_000),
                None,
                None,
            )
            .unwrap();
        let next = confirmation_next_step(
            ConfirmationDecision::ConfirmScope,
            &[sales],
            &BTreeSet::from(["SALES"]),
        );
        let mut import_batch = batch();
        if next == ImportBusinessConfirmationNextStep::StartApply {
            import_batch
                .advance(LegacyImportBatchStatus::ReadyToApply)
                .unwrap();
        }

        assert_eq!(next, ImportBusinessConfirmationNextStep::StartApply);
        assert_eq!(import_batch.status, LegacyImportBatchStatus::ReadyToApply);
    }

    #[test]
    fn start_apply_is_the_only_action_that_starts_background_job() {
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::ReadyToApply;
        let mut job = build_background_job(&import_batch, "admin-1").unwrap();

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
        let mut job = build_background_job(&import_batch, "admin-1").unwrap();
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
        let mut job = build_background_job(&import_batch, "admin-1").unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_progress(1, 1, 1, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        job.mark_partially_succeeded().unwrap();
        job.mark_succeeded(Instant::from_unix_secs(1_700_000_200))
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
    fn execution_receipt_is_stable_and_rejects_request_reuse() {
        let command = execution_command(ImportExecutionAction::StartApply);
        let fingerprint = import_execution_fingerprint(&command);
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
        assert!(
            !import_execution_audit_id("user-1", "execute", "batch-1", "secret-request")
                .contains("secret-request")
        );
    }

    #[test]
    fn execution_receipt_replay_allows_later_background_progress() {
        let mut import_batch = batch();
        import_batch.status = LegacyImportBatchStatus::Completed;
        import_batch.base.version = 99;
        let mut job = build_background_job(&import_batch, "admin-1").unwrap();
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

    #[test]
    fn idempotency_receipt_rejects_same_key_with_different_command() {
        let fingerprint = confirmation_completion_fingerprint(&completion_command());
        let receipt = ConfirmationCompletionReceipt {
            result_status: ImportBusinessConfirmationResultStatus::Confirmed,
            task_version: 4,
            batch_version: 5,
            next_step: ImportBusinessConfirmationNextStep::AwaitOtherConfirmations,
        };
        let message = confirmation_completion_receipt_message(&fingerprint, receipt);

        assert_eq!(
            parse_confirmation_completion_receipt(&message, &fingerprint).unwrap(),
            receipt
        );
        assert!(parse_confirmation_completion_receipt(&message, &"0".repeat(64)).is_err());
        assert!(
            !confirmation_completion_audit_id("user-1", "complete", "work-item-1", "secret-key")
                .contains("secret-key")
        );
    }
}
