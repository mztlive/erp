//! 域 D23 `mall_sync` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 快照落盘（快照 + 作业进度 + 审计日志）→ 跨集合事务；事实键唯一索引
//!   `(source_system_id, external_order_key, source_updated_at)` 为权威去重，
//!   重复推送与迟到快照在写入前判定跳过（§6.13）；
//! - 作业完成（作业终态 + 水位前移/新建 + 审计日志）→ 跨集合事务；只有
//!   `Success` 才前移水位（§8.4 第 2 条：分页全部安全持久化后才前移水位）；
//! - 核对作业创建（作业 + 差异明细 + 审计日志）→ 跨集合事务；
//! - 差异明细/映射任务处理（明细 + 审计日志）→ 跨集合事务；
//! - 其余单集合查询传 `&mut NoTransaction`。
//!
//! 跨域协作只经 `DatabaseExt` 调对方 Repository（P3-service-api §2）：
//! - D01 `source_registry`：作业/核对创建时校验来源商城存在；
//! - D13 `sales_order`：核对明细携带 ERP 销售单 ID 时校验销售单存在；
//! - D08 `customer`：沿 ERP 销售单的客户账号校验客户存在。
//!
//! 幂等约定（AGENTS.md 外部依赖容错）：快照事实键、核对批次号唯一索引为权威
//! 去重；作业完成与差异处理在终态重复提交时按幂等返回，不产生重复事实。

use database::{
    AccessControlExt, CustomerExt, MallSyncExt, NoTransaction, SalesOrderExt, SourceRegistryExt,
    Transactional,
};
use entities::common::time::Instant;
use entities::ids::{
    MallSalesOrderSnapshotId, MallSalesReconciliationItemId, MallSalesReconciliationJobId,
    MallSalesSyncCursorId, MallSalesSyncJobId, MasterMappingTaskId,
};
use entities::mall_sync::{
    ExternalOrderKey, MallSalesOrderSnapshot, MallSalesOrderSnapshotData, MallSalesReconciliationItem,
    MallSalesReconciliationItemData, MallSalesReconciliationJob, MallSalesReconciliationJobData,
    MallSalesSyncCursor, MallSalesSyncJob, MallSalesSyncJobData, MallSalesSyncJobStatus,
    MallSalesSyncJobType, MasterMappingTask, MasterMappingTaskData, ReconciliationItemStatus,
    ReconciliationJobStatus,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub mod dto;

pub use self::dto::{
    CompleteMallSalesSyncJobRequest, CreateMallSalesReconciliationJobRequest, CreateMallSalesSyncJobRequest,
    CreateMasterMappingTaskRequest, IngestMallSalesOrderSnapshotsRequest,
    IngestMallSalesOrderSnapshotsResult, MallSalesOrderSnapshotListParams, MallSalesOrderSnapshotView,
    MallSalesReconciliationItemListParams, MallSalesReconciliationItemView,
    MallSalesReconciliationJobListParams, MallSalesReconciliationJobView, MallSalesSyncCursorView,
    MallSalesSyncJobListParams, MallSalesSyncJobView, MasterMappingTaskListParams, MasterMappingTaskView,
    PageView, ResolveMallSalesReconciliationItemRequest, ResolveMasterMappingTaskRequest,
};
use self::dto::{
    MallSalesSyncJobListQuery, SortDir, SyncJobOutcome, SALES_ORDER_CUSTOMER_MISSING_MESSAGE,
    SALES_ORDER_NOT_FOUND_MESSAGE, SOURCE_SYSTEM_NOT_FOUND_MESSAGE,
};

/// 同步作业列表筛选条件类型（经 `MallSyncExt` 关联类型跨 crate 可达）。
type MallSalesSyncJobFilter = <mongodb::Database as MallSyncExt>::MallSalesSyncJobFilter;
/// 快照列表筛选条件类型。
type MallSalesOrderSnapshotFilter = <mongodb::Database as MallSyncExt>::MallSalesOrderSnapshotFilter;
/// 核对作业列表筛选条件类型。
type MallSalesReconciliationJobFilter = <mongodb::Database as MallSyncExt>::MallSalesReconciliationJobFilter;
/// 核对差异明细列表筛选条件类型。
type MallSalesReconciliationItemFilter =
    <mongodb::Database as MallSyncExt>::MallSalesReconciliationItemFilter;
/// 映射任务列表筛选条件类型。
type MasterMappingTaskFilter = <mongodb::Database as MallSyncExt>::MasterMappingTaskFilter;

/// 商城卡券销售单同步服务。
///
/// 提供同步作业、水位游标、销售单快照、核对作业与映射任务的创建、查询与状态推进编排。
pub struct MallSyncService {
    db: Database,
}

impl MallSyncService {
    /// 创建商城同步服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 创建同步作业。
    ///
    /// 来源商城经 D01 仓储校验；同一来源商城只允许一个运行中的增量任务
    /// （§6.13），存在时返回 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建同步作业的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源商城不存在
    /// * `ConflictError` - 该来源商城已有运行中的增量任务
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_sync_job(
        &self,
        req: CreateMallSalesSyncJobRequest,
        actor: &AuditActor,
    ) -> Result<MallSalesSyncJobView> {
        req.validate()?;
        self.ensure_source_system(&req.source_system_id).await?;
        if req.job_type == MallSalesSyncJobType::Incremental
            && self
                .db
                .mall_sales_sync_jobs()
                .find_running_incremental_by_source(&req.source_system_id, &mut NoTransaction)
                .await?
                .is_some()
        {
            return Err(Error::ConflictError(
                "该来源商城已有运行中的增量任务，禁止并发推进水位".to_string(),
            ));
        }

        let job = MallSalesSyncJob::new(
            MallSalesSyncJobId::new(next_id()),
            MallSalesSyncJobData {
                source_system_id: req.source_system_id,
                job_type: req.job_type,
                range_start: req.range_start,
                range_end: req.range_end,
                started_at: Instant::now(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "mall_sales_sync_job.create",
            "mall_sales_sync_job",
            job.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let job_for_tx = job.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_sales_sync_jobs().create(&job_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(job.into())
    }

    /// 分页查询同步作业列表。
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
    pub async fn sync_job_list(
        &self,
        params: &MallSalesSyncJobListParams,
    ) -> Result<PageView<MallSalesSyncJobView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = self.sync_job_filter_of(&query);
        let page = self
            .db
            .mall_sales_sync_jobs()
            .search_mall_sales_sync_jobs(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| MallSalesSyncJobView {
                id: row.id,
                source_system_id: row.source_system_id.to_string(),
                job_type: row.job_type,
                range_start: row.range_start,
                range_end: row.range_end,
                started_at: row.started_at,
                finished_at: row.finished_at,
                status: row.status,
                page_count: row.page_count,
                item_count: row.item_count,
                error_count: row.error_count,
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

    /// 查询同步作业详情。
    ///
    /// # 参数
    /// * `id` - 同步作业 ID
    ///
    /// # 返回
    /// 返回同步作业的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 作业不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn sync_job_detail(&self, id: &str) -> Result<MallSalesSyncJobView> {
        let job = self
            .db
            .mall_sales_sync_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("同步作业不存在".to_string()))?;
        Ok(job.into())
    }

    /// 落盘一页商城销售单快照（幂等）。
    ///
    /// 按事实键去重：重复推送（相同 `source_updated_at`）与早于最新快照的迟到
    /// 数据直接跳过（§6.13），不产生重复快照；同时推进作业处理计数。
    /// 快照内容创建后不可修改，映射状态保持待映射。
    ///
    /// # 参数
    /// * `req` - 落盘请求（作业 + 本页快照）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回本页落盘与跳过计数。
    ///
    /// # 错误
    /// * `NotFound` - 同步作业不存在
    /// * `BusinessLogicError` - 作业不在运行中
    /// * `ConflictError` - 并发重复推送触发唯一索引冲突
    pub async fn ingest_snapshots(
        &self,
        req: IngestMallSalesOrderSnapshotsRequest,
        actor: &AuditActor,
    ) -> Result<IngestMallSalesOrderSnapshotsResult> {
        req.validate()?;
        let job = self
            .db
            .mall_sales_sync_jobs()
            .find_by_id(req.sync_job_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("同步作业不存在".to_string()))?;
        if job.status != MallSalesSyncJobStatus::Running {
            return Err(Error::BusinessLogicError(
                "同步作业不在运行中，禁止落盘快照".to_string(),
            ));
        }
        let now = Instant::now();
        let mut accepted = Vec::new();
        let mut skipped = 0u64;
        for item in &req.items {
            let key = ExternalOrderKey::from_trimmed(&item.external_order_no);
            if self
                .db
                .mall_sales_order_snapshots()
                .find_by_fact_key(
                    &job.source_system_id,
                    &key,
                    item.source_updated_at,
                    &mut NoTransaction,
                )
                .await?
                .is_some()
            {
                skipped += 1;
                continue;
            }
            if self
                .snapshot_is_stale(&job.source_system_id, &key, item.source_updated_at)
                .await?
            {
                skipped += 1;
                continue;
            }
            accepted.push(MallSalesOrderSnapshot::new(
                MallSalesOrderSnapshotId::new(next_id()),
                MallSalesOrderSnapshotData {
                    source_system_id: job.source_system_id.clone(),
                    external_order_no: item.external_order_no.clone(),
                    source_updated_at: item.source_updated_at,
                    content_hash: item.content_hash.clone(),
                    source_status_code: item.source_status_code.clone(),
                    normalized_snapshot: item.normalized_snapshot.clone(),
                    raw_payload_reference: item.raw_payload_reference.clone(),
                    observed_at: now,
                    sync_job_id: MallSalesSyncJobId::new(job.base.id.clone()),
                },
            )?);
        }
        if accepted.is_empty() {
            return Ok(IngestMallSalesOrderSnapshotsResult {
                accepted: 0,
                skipped,
                snapshot_ids: Vec::new(),
            });
        }

        let mut job = job;
        job.record_progress(1, accepted.len() as u64, 0)?;
        let accepted_count = accepted.len() as u64;
        let snapshot_ids = accepted.iter().map(|snapshot| snapshot.base.id.clone()).collect();
        let audit = actor.clone().resource_log(
            "mall_sales_order_snapshot.create",
            "mall_sales_order_snapshot",
            job.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut job_for_tx = job.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    for snapshot in &accepted {
                        db.mall_sales_order_snapshots().create(snapshot, session).await?;
                    }
                    db.mall_sales_sync_jobs().update(&mut job_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(IngestMallSalesOrderSnapshotsResult {
            accepted: accepted_count,
            skipped,
            snapshot_ids,
        })
    }

    /// 分页查询快照列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn snapshot_list(
        &self,
        params: &MallSalesOrderSnapshotListParams,
    ) -> Result<PageView<MallSalesOrderSnapshotView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MallSalesOrderSnapshotFilter {
            source_system_id: query.source_system_id,
            mapping_status: query.mapping_status,
            observed_at_from: query.observed_at_from,
            observed_at_to: query.observed_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_sales_order_snapshots()
            .search_mall_sales_order_snapshots(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| MallSalesOrderSnapshotView {
                id: row.id,
                source_system_id: row.source_system_id.to_string(),
                external_order_no: row.external_order_no,
                source_updated_at: row.source_updated_at,
                content_hash: row.content_hash,
                source_status_code: row.source_status_code,
                observed_at: row.observed_at,
                mapping_status: row.mapping_status,
                applied_sales_order_revision_id: row.applied_sales_order_revision_id.map(|id| id.to_string()),
                sync_job_id: row.sync_job_id.to_string(),
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

    /// 完成同步作业并推进水位。
    ///
    /// 仅运行中作业可完成；`Success` 要求错误计数为零（实体校验）。水位只在
    /// `Success` 时前移（§8.4 第 2 条：分页全部安全持久化后才前移水位）：
    /// 已有游标按区间止单调前移（相等水位幂等），期初基线完成后新建游标且
    /// 初值取基线拉取开始时间。同一作业重复提交相同终态按幂等返回。
    ///
    /// # 参数
    /// * `id` - 同步作业 ID
    /// * `req` - 完成请求（终态结果）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回完成后的同步作业视图。
    ///
    /// # 错误
    /// * `NotFound` - 作业不存在
    /// * `ConflictError` - 作业已终态且结果不一致
    /// * `BusinessLogicError` - 成功结果携带错误计数等实体校验失败
    pub async fn complete_sync_job(
        &self,
        id: &str,
        req: CompleteMallSalesSyncJobRequest,
        actor: &AuditActor,
    ) -> Result<MallSalesSyncJobView> {
        req.validate()?;
        let mut job = self
            .db
            .mall_sales_sync_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("同步作业不存在".to_string()))?;
        let outcome = outcome_of(req.outcome);
        if job.status != MallSalesSyncJobStatus::Running {
            if job.status == outcome {
                tracing::info!(job_id = id, status = ?job.status, "作业已终态，按幂等返回");
                return Ok(job.into());
            }
            return Err(Error::ConflictError("同步作业已终态，结果不一致".to_string()));
        }
        job.finish(outcome, Instant::now())?;
        let cursor_action = if outcome == MallSalesSyncJobStatus::Success {
            self.cursor_after_success(&mut job).await?
        } else {
            CursorAction::None
        };
        let audit = actor.clone().resource_log(
            "mall_sales_sync_job.complete",
            "mall_sales_sync_job",
            job.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut job_for_tx = job.clone();
        let updated_job = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_sales_sync_jobs().update(&mut job_for_tx, session).await?;
                    match cursor_action {
                        CursorAction::Create(cursor) => {
                            db.mall_sales_sync_cursors().create(&cursor, session).await?;
                        }
                        CursorAction::Advance(mut cursor) => {
                            db.mall_sales_sync_cursors().update(&mut cursor, session).await?;
                        }
                        CursorAction::None => {}
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<MallSalesSyncJob, crate::errors::Error>(job_for_tx)
                })
            })
            .await?;

        Ok(updated_job.into())
    }

    /// 查询同步水位游标（单来源商城单行）。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    ///
    /// # 返回
    /// 返回匹配的水位游标；尚未建立时返回 `None`（数据为 `null`）。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn sync_cursor_detail(
        &self,
        source_system_id: &str,
    ) -> Result<Option<MallSalesSyncCursorView>> {
        let cursor = self
            .db
            .mall_sales_sync_cursors()
            .find_by_source(
                &entities::ids::SourceSystemId::new(source_system_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(cursor.map(Into::into))
    }

    /// 创建核对作业并写入差异明细（原子）。
    ///
    /// 核对批次号唯一：重复提交按幂等返回既有作业。携带 ERP 销售单 ID 的明细
    /// 经 D13 校验销售单存在、经 D08 沿销售单客户账号校验客户存在；作业创建即
    /// 完成（差异数量大于零 → `HasDifference`，§6.13）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或既有）核对作业的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源商城/ERP 销售单/客户账号不存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_reconciliation_job(
        &self,
        req: CreateMallSalesReconciliationJobRequest,
        actor: &AuditActor,
    ) -> Result<MallSalesReconciliationJobView> {
        req.validate()?;
        self.ensure_source_system(&req.source_system_id).await?;
        if let Some(existing) = self
            .db
            .mall_sales_reconciliation_jobs()
            .find_by_job_no(&req.job_no, &mut NoTransaction)
            .await?
        {
            tracing::info!(job_no = %req.job_no, "核对作业已存在，按幂等返回");
            return Ok(existing.into());
        }
        self.ensure_erp_sides_exist(&req).await?;

        let job_id = MallSalesReconciliationJobId::new(next_id());
        let mut job = MallSalesReconciliationJob::new(
            job_id.clone(),
            MallSalesReconciliationJobData {
                source_system_id: req.source_system_id,
                job_no: req.job_no,
                source_list_as_of: req.source_list_as_of,
                started_at: Instant::now(),
            },
        )?;
        job.record_counts(req.source_count, req.erp_count, req.items.len() as u64)?;
        let finished_at = Instant::now();
        job.finish(ReconciliationJobStatus::HasDifference, finished_at)?;
        let items = req
            .items
            .iter()
            .map(|item| {
                MallSalesReconciliationItem::new(
                    MallSalesReconciliationItemId::new(next_id()),
                    MallSalesReconciliationItemData {
                        reconciliation_job_id: job_id.clone(),
                        external_order_no: item.external_order_no.clone(),
                        source_status_code: item.source_status_code.clone(),
                        source_updated_at: item.source_updated_at,
                        source_content_hash: item.source_content_hash.clone(),
                        sales_order_id: item.sales_order_id.clone(),
                        erp_revision_id: item.erp_revision_id.clone(),
                        erp_content_hash: item.erp_content_hash.clone(),
                        difference_type: item.difference_type,
                    },
                )
                .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        let audit = actor.clone().resource_log(
            "mall_sales_reconciliation_job.create",
            "mall_sales_reconciliation_job",
            job.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let job_for_tx = job.clone();
        let items_for_tx = items.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_sync()
                        .create_reconciliation_job_with_items(&job_for_tx, &items_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(job.into())
    }

    /// 分页查询核对作业列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn reconciliation_job_list(
        &self,
        params: &MallSalesReconciliationJobListParams,
    ) -> Result<PageView<MallSalesReconciliationJobView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MallSalesReconciliationJobFilter {
            source_system_id: query.source_system_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_sales_reconciliation_jobs()
            .search_mall_sales_reconciliation_jobs(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| MallSalesReconciliationJobView {
                id: row.id,
                source_system_id: row.source_system_id.to_string(),
                job_no: row.job_no,
                source_list_as_of: row.source_list_as_of,
                source_count: row.source_count,
                erp_count: row.erp_count,
                difference_count: row.difference_count,
                status: row.status,
                started_at: row.started_at,
                finished_at: row.finished_at,
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

    /// 分页查询核对差异明细列表（按核对作业）。
    ///
    /// # 参数
    /// * `job_id` - 所属核对作业
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `NotFound` - 核对作业不存在
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn reconciliation_item_list(
        &self,
        job_id: &str,
        params: &MallSalesReconciliationItemListParams,
    ) -> Result<PageView<MallSalesReconciliationItemView>> {
        if self
            .db
            .mall_sales_reconciliation_jobs()
            .find_by_id(job_id, &mut NoTransaction)
            .await?
            .is_none()
        {
            return Err(Error::NotFound("核对作业不存在".to_string()));
        }
        params.validate()?;
        let query = params.normalized()?;
        let filter = MallSalesReconciliationItemFilter {
            reconciliation_job_id: Some(MallSalesReconciliationJobId::new(job_id.to_string())),
            status: query.status,
            difference_type: query.difference_type,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_sales_reconciliation_items()
            .search_mall_sales_reconciliation_items(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| MallSalesReconciliationItemView {
                id: row.id,
                reconciliation_job_id: row.reconciliation_job_id.to_string(),
                external_order_no: row.external_order_no,
                source_status_code: row.source_status_code,
                source_updated_at: row.source_updated_at,
                difference_type: row.difference_type,
                status: row.status,
                single_order_sync_job_id: row.single_order_sync_job_id.map(|id| id.to_string()),
                resolution: row.resolution,
                resolved_by: row.resolved_by,
                resolved_at: row.resolved_at,
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

    /// 处理核对差异明细（人工解决或确认无误）。
    ///
    /// 明细已终态时重复提交按幂等返回（不产生重复事实，§6.13）。
    ///
    /// # 参数
    /// * `id` - 差异明细 ID
    /// * `req` - 处理请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回处理后的差异明细视图。
    ///
    /// # 错误
    /// * `NotFound` - 明细不存在
    /// * `ValidationError` - 请求体校验失败（解决缺少结论等）
    pub async fn resolve_reconciliation_item(
        &self,
        id: &str,
        req: ResolveMallSalesReconciliationItemRequest,
        actor: &AuditActor,
    ) -> Result<MallSalesReconciliationItemView> {
        req.validate()?;
        let mut item = self
            .db
            .mall_sales_reconciliation_items()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("核对差异明细不存在".to_string()))?;
        if matches!(
            item.status,
            ReconciliationItemStatus::Resolved | ReconciliationItemStatus::ConfirmedNoDifference
        ) {
            tracing::info!(item_id = id, status = ?item.status, "差异明细已终态，按幂等返回");
            return Ok(item.into());
        }
        let now = Instant::now();
        match req.kind {
            dto::ResolveItemKind::Resolve => {
                let resolution = req
                    .resolution
                    .ok_or_else(|| Error::ValidationError("人工解决必须提供处理结论".to_string()))?;
                item.resolve(resolution, actor.id().to_string(), now)?;
            }
            dto::ResolveItemKind::ConfirmNoDifference => {
                item.confirm_no_difference(actor.id().to_string(), now)?;
            }
        }
        let audit = actor.clone().resource_log(
            "mall_sales_reconciliation_item.resolve",
            "mall_sales_reconciliation_item",
            item.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut item_for_tx = item.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_sales_reconciliation_items()
                        .update(&mut item_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(item.into())
    }

    /// 分页查询映射任务列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mapping_task_list(
        &self,
        params: &MasterMappingTaskListParams,
    ) -> Result<PageView<MasterMappingTaskView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MasterMappingTaskFilter {
            source_snapshot_id: query.source_snapshot_id,
            mapping_type: query.mapping_type,
            status: query.status,
            owner_role: query.owner_role,
            owner_user_id: query.owner_user_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .master_mapping_tasks()
            .search_master_mapping_tasks(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| MasterMappingTaskView {
                id: row.id,
                source_snapshot_id: row.source_snapshot_id.to_string(),
                mapping_type: row.mapping_type,
                status: row.status,
                owner_role: row.owner_role,
                owner_user_id: row.owner_user_id,
                resolution: row.resolution,
                resolved_at: row.resolved_at,
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

    /// 创建映射任务（单集合写入，无事务）。
    ///
    /// 同一快照、映射类型只允许一个进行中任务（§6.13）：先查后插 + 部分唯一
    /// 索引兜底，重复创建返回 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建映射任务的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 快照不存在
    /// * `ConflictError` - 该快照的同类映射任务已存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_mapping_task(
        &self,
        req: CreateMasterMappingTaskRequest,
        actor: &AuditActor,
    ) -> Result<MasterMappingTaskView> {
        req.validate()?;
        if self
            .db
            .mall_sales_order_snapshots()
            .find_by_id(req.source_snapshot_id.as_ref(), &mut NoTransaction)
            .await?
            .is_none()
        {
            return Err(Error::NotFound("商城销售单快照不存在".to_string()));
        }
        if self
            .db
            .master_mapping_tasks()
            .find_pending_by_snapshot_and_type(&req.source_snapshot_id, req.mapping_type, &mut NoTransaction)
            .await?
            .is_some()
        {
            return Err(Error::ConflictError(
                "该快照的同类映射任务已存在，禁止重复创建".to_string(),
            ));
        }

        let task = MasterMappingTask::new(
            MasterMappingTaskId::new(next_id()),
            MasterMappingTaskData {
                source_snapshot_id: req.source_snapshot_id,
                mapping_type: req.mapping_type,
                owner_role: req.owner_role,
                owner_user_id: req.owner_user_id,
            },
        )?;
        let audit = actor.clone().resource_log(
            "master_mapping_task.create",
            "master_mapping_task",
            task.base.id.clone(),
        )?;

        self.db
            .master_mapping_tasks()
            .create(&task, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        Ok(task.into())
    }

    /// 处理映射任务（已解决或无法处理）。
    ///
    /// 任务已终态时重复提交按幂等返回（不产生重复事实，§6.13）。
    ///
    /// # 参数
    /// * `id` - 映射任务 ID
    /// * `req` - 处理请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回处理后的映射任务视图。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn resolve_mapping_task(
        &self,
        id: &str,
        req: ResolveMasterMappingTaskRequest,
        actor: &AuditActor,
    ) -> Result<MasterMappingTaskView> {
        req.validate()?;
        let mut task = self
            .db
            .master_mapping_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("映射任务不存在".to_string()))?;
        if matches!(
            task.status,
            entities::mall_sync::MappingTaskStatus::Resolved
                | entities::mall_sync::MappingTaskStatus::Unresolvable
                | entities::mall_sync::MappingTaskStatus::Closed
        ) {
            tracing::info!(task_id = id, status = ?task.status, "映射任务已终态，按幂等返回");
            return Ok(task.into());
        }
        let now = Instant::now();
        match req.kind {
            dto::ResolveTaskKind::Resolved => task.resolve(req.resolution.clone(), now)?,
            dto::ResolveTaskKind::Unresolvable => task.mark_unresolvable(req.resolution.clone(), now)?,
        }
        let audit = actor.clone().resource_log(
            "master_mapping_task.resolve",
            "master_mapping_task",
            task.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut task_for_tx = task.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.master_mapping_tasks()
                        .update(&mut task_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(task.into())
    }

    /// 判定快照是否迟到（早于同一来源单最新快照）。
    ///
    /// 数据模型 §6.13：同一来源单收到更早 `source_updated_at` 的快照直接丢弃，
    /// 不持久化、不推进当前版本。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    /// * `external_order_key` - 来源单二进制比较键
    /// * `source_updated_at` - 商城更新时间
    ///
    /// # 返回
    /// 迟到返回 `true`（应丢弃）。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn snapshot_is_stale(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        external_order_key: &ExternalOrderKey,
        source_updated_at: entities::common::time::Instant,
    ) -> Result<bool> {
        let latest = self
            .db
            .mall_sales_order_snapshots()
            .find_latest_by_order(source_system_id, external_order_key, &mut NoTransaction)
            .await?;
        Ok(latest.is_some_and(|snapshot| snapshot.source_updated_at > source_updated_at))
    }

    /// 校验来源商城存在（D01 仓储读取）。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    ///
    /// # 错误
    /// * `NotFound` - 来源商城不存在
    async fn ensure_source_system(&self, source_system_id: &entities::ids::SourceSystemId) -> Result<()> {
        self.db
            .source_systems()
            .find_by_id(source_system_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound(SOURCE_SYSTEM_NOT_FOUND_MESSAGE.to_string()))?;
        Ok(())
    }

    /// 校验核对明细的 ERP 侧引用（D13 + D08 仓储读取）。
    ///
    /// 携带 ERP 销售单 ID 的明细要求：销售单存在（D13），且销售单的客户账号
    /// 存在（D08 沿 `sales_order.customer_id`）。
    ///
    /// # 参数
    /// * `req` - 核对作业创建请求
    ///
    /// # 错误
    /// * `NotFound` - 销售单或客户账号不存在
    async fn ensure_erp_sides_exist(&self, req: &CreateMallSalesReconciliationJobRequest) -> Result<()> {
        for item in &req.items {
            let Some(sales_order_id) = item.sales_order_id.clone() else {
                continue;
            };
            let order = self
                .db
                .sales_orders()
                .find_by_id(sales_order_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound(SALES_ORDER_NOT_FOUND_MESSAGE.to_string()))?;
            if self
                .db
                .customer_accounts()
                .find_by_id(order.customer_id.as_ref(), &mut NoTransaction)
                .await?
                .is_none()
            {
                return Err(Error::NotFound(SALES_ORDER_CUSTOMER_MISSING_MESSAGE.to_string()));
            }
        }
        Ok(())
    }

    /// 作业成功后推进或新建水位游标。
    ///
    /// 已有游标按区间止单调前移（`move_forward` 相等水位幂等）；无游标时
    /// （期初基线完成）新建游标，初值取基线拉取开始时间（erp-phase-1 §8.4）。
    /// 单号补拉等无区间任务不推进水位。
    ///
    /// # 参数
    /// * `job` - 同步作业（内存态，成功后状态为 `Success`）
    ///
    /// # 返回
    /// 返回待写入的游标动作；无需推进时返回 `None`。
    ///
    /// # 错误
    /// 水位回退或区间缺失时返回错误。
    async fn cursor_after_success(&self, job: &mut MallSalesSyncJob) -> Result<CursorAction> {
        let Some(range_end) = job.range_end else {
            return Ok(CursorAction::None);
        };
        if let Some(mut cursor) = self
            .db
            .mall_sales_sync_cursors()
            .find_by_source(&job.source_system_id, &mut NoTransaction)
            .await?
        {
            cursor
                .move_forward(range_end, MallSalesSyncJobId::new(job.base.id.clone()))
                .map_err(|_| Error::ConflictError("同步水位不得回退".to_string()))?;
            return Ok(CursorAction::Advance(cursor));
        }
        let initial_water = job
            .range_start
            .ok_or_else(|| Error::BusinessLogicError("新建水位需要同步区间起点".to_string()))?;
        let mut cursor = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new(next_id()),
            job.source_system_id.clone(),
            initial_water,
        );
        // 登记最近成功任务：水位保持基线拉取开始时间不变（相等水位幂等），
        // 只补充 `last_success_job_id`（数据模型 §6.13）。
        cursor.move_forward(initial_water, MallSalesSyncJobId::new(job.base.id.clone()))?;
        Ok(CursorAction::Create(cursor))
    }

    /// 构造同步作业列表筛选条件。
    ///
    /// # 参数
    /// * `query` - 归一化查询参数
    ///
    /// # 返回
    /// 返回仓储筛选条件。
    fn sync_job_filter_of(&self, query: &MallSalesSyncJobListQuery) -> MallSalesSyncJobFilter {
        MallSalesSyncJobFilter {
            source_system_id: query.source_system_id.clone(),
            job_type: query.job_type,
            status: query.status,
            started_at_from: query.started_at_from,
            started_at_to: query.started_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        }
    }
}

/// 将请求终态结果映射为作业状态。
///
/// # 参数
/// * `outcome` - 请求终态结果
///
/// # 返回
/// 返回对应作业状态。
fn outcome_of(outcome: SyncJobOutcome) -> MallSalesSyncJobStatus {
    match outcome {
        SyncJobOutcome::Success => MallSalesSyncJobStatus::Success,
        SyncJobOutcome::PartialFailure => MallSalesSyncJobStatus::PartialFailure,
        SyncJobOutcome::Failed => MallSalesSyncJobStatus::Failed,
    }
}

/// 作业完成后的水位游标写入动作。
enum CursorAction {
    /// 新建游标（期初基线完成，初值取基线拉取开始时间）。
    Create(MallSalesSyncCursor),
    /// 前移既有游标（版本 CAS 写）。
    Advance(MallSalesSyncCursor),
    /// 不推进水位（失败/部分失败或单号补拉等无区间任务）。
    None,
}
