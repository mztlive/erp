//! 域 D23 `mall_sync` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 快照落盘（快照 + 作业进度 + 审计日志 + 来源单水位）→ 跨集合事务；
//!   Repository 批量读取 exact/latest，Entity 分类 Duplicate/Stale/Accept，
//!   水位 CAS 阻止并发旧版本落盘；事实键唯一索引仍为等时去重权威（§6.13）；
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
    AccessControlExt, CatalogExt, ContractExt, CustomerExt, Executor, MallSyncExt, NoTransaction, PartyExt,
    ReceivableExt, SalesOrderExt, SourceRegistryExt, Transactional, WorkItemExt,
};
use entities::catalog::EnableStatus;
use entities::common::time::{BusinessDate, Instant};
use entities::contract::ContractStatus;
use entities::ids::{
    ExternalIdentityMapId, ExternalIdentityTargetId, MallSalesOrderSnapshotId, MallSalesReconciliationItemId,
    MallSalesReconciliationJobId, MallSalesSyncCursorId, MallSalesSyncJobId, MasterMappingTaskId, WorkItemId,
};
use entities::mall_sync::{
    MallSalesOrderSnapshot, MallSalesReconciliationItem, MallSalesReconciliationItemData,
    MallSalesReconciliationJob, MallSalesReconciliationJobData, MallSalesSyncCursor, MallSalesSyncJob,
    MallSalesSyncJobData, MallSalesSyncJobStatus, MallSalesSyncJobType, MallSnapshotReapplyOperation,
    MallSnapshotReapplyOperationData, MallSyncCommandIdentity, MallSyncTimeRange, MallSyncTriggerSource,
    MappingSourceIdentity, MappingTaskStatus, MappingTaskType, MasterMappingTask, MasterMappingTaskData,
    ReconciliationJobStatus, SyncJobCompletionDisposition,
};
use entities::source_registry::{
    ExternalIdentityMap, ExternalIdentityMapData, ExternalIdentityTarget, ExternalIdentityTargetData,
    MallSyncStage, MappingStatus, SourceSystemStatus, SourceSystemType, TargetStatus,
};
use entities::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use id_generator::next_id;
use mongodb::Database;
use serde::Serialize;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub mod dto;
mod snapshot_ingest;

pub use self::dto::{
    CompleteMallSalesSyncJobRequest, ConfirmMappingCommand, ConfirmMappingResult,
    CreateMallSalesReconciliationJobRequest, CreateMasterMappingTaskRequest, GovernanceActionResult,
    IngestMallSalesOrderSnapshotsRequest, IngestMallSalesOrderSnapshotsResult,
    MallSalesOrderSnapshotListParams, MallSalesOrderSnapshotView, MallSalesReconciliationItemListParams,
    MallSalesReconciliationItemView, MallSalesReconciliationJobListParams, MallSalesReconciliationJobView,
    MallSalesSyncCursorView, MallSalesSyncJobListParams, MallSalesSyncJobView, MasterMappingTaskDetailParams,
    MasterMappingTaskListParams, MasterMappingTaskView, PageView, ReapplyMallSnapshotCommand,
    ReapplyOperationView, RequestSourceFixCommand, RequestSourceFixResult,
    ResolveMallSalesReconciliationItemRequest, RetryMallSalesSyncJobRequest, TriggerMallSyncCommand,
};
use self::dto::{
    MallSalesSyncJobListQuery, MappingActionBlockerView, MappingCandidateTargetView,
    MappingCurrentTargetView, MappingResolutionHistoryView, MappingSourceEvidenceView,
    MappingTaskWorkItemView, OwnerRoutingState, SortDir, SyncJobOutcome,
    SALES_ORDER_CUSTOMER_MISSING_MESSAGE, SALES_ORDER_NOT_FOUND_MESSAGE, SOURCE_SYSTEM_NOT_FOUND_MESSAGE,
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
type ContractFilter = <mongodb::Database as ContractExt>::ContractFilter;
type VoucherCategoryProfileRevisionFilter =
    <mongodb::Database as CatalogExt>::VoucherCategoryProfileRevisionFilter;

const W17_OBJECT_TYPE: &str = "MASTER_MAPPING_TASK";
const W17_OWNER_ORGANIZATION: &str = "company";
const W17_RECEIPT_PREFIX: &str = "w17-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
const MALL_SYNC_OVERLAP_SECONDS: i64 = 300;

struct MappingCommandContext {
    task: MasterMappingTask,
    snapshot: MallSalesOrderSnapshot,
    work_item: WorkItem,
}

/// 确认映射事务的命令、审计与谱系标识。
///
/// # 用途
/// 将确认命令、操作人、幂等审计与谱系主键打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 谱系 ID 必须与命令引用或服务端生成值一致。
struct ConfirmMappingTxInput {
    /// 确认映射命令。
    command: ConfirmMappingCommand,
    /// 审计操作人。
    actor: AuditActor,
    /// 期望任务版本。
    expected_task_version: u64,
    /// 请求摘要。
    fingerprint: String,
    /// 幂等审计主键。
    audit_id: String,
    /// 谱系主键。
    map_id: String,
    /// 目标主键。
    target_id: String,
}

struct MappingLineageWrite {
    mapping: ExternalIdentityMap,
    target: ExternalIdentityTarget,
    expired_targets: Vec<ExternalIdentityTarget>,
    is_new_mapping: bool,
}

struct TriggerJobSpec {
    job_type: MallSalesSyncJobType,
    range_start: Option<Instant>,
    range_end: Option<Instant>,
    external_order_no: Option<String>,
    trigger_source: MallSyncTriggerSource,
    trigger_reason: Option<String>,
    triggered_by: Option<String>,
    source_job_id: Option<MallSalesSyncJobId>,
}

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

    /// 沿失败作业的服务端事实创建一次重试作业。
    ///
    /// # 参数
    /// * `failed_job_id` - 待重试作业 ID
    /// * `request` - 仅包含理由、可选水位版本与幂等键的重试请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回原子创建的重试作业视图。
    ///
    /// # 错误
    /// 原作业不存在、状态不可重试、来源阶段不可写或版本冲突时返回错误。
    pub async fn retry_sync_job(
        &self,
        failed_job_id: &str,
        request: RetryMallSalesSyncJobRequest,
        actor: &AuditActor,
    ) -> Result<MallSalesSyncJobView> {
        let failed_job_id = required_trigger_text(failed_job_id, "失败作业 ID")?;
        let original = self
            .db
            .mall_sales_sync_jobs()
            .find_by_id(&failed_job_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待重试同步作业不存在".to_string()))?;

        self.trigger_sync_job(
            TriggerMallSyncCommand::RetryFailedJob {
                source_system_id: original.source_system_id,
                execution_stage: MallSyncStage::FirstPhaseMallOwned,
                failed_job_id,
                reason: request.reason,
                base_cursor_version: request.base_cursor_version,
                idempotency_key: request.idempotency_key,
            },
            actor,
        )
        .await
    }

    /// 按 W17 强类型触发命令创建同步作业。
    ///
    /// Service 重读来源阶段、水位与失败作业；调用方不能提供查询范围或伪造
    /// 来源身份。人工动作必须携带理由，按单补拉必须携带原来源单号。
    ///
    /// # 参数
    /// * `command` - 阶段强判别触发命令
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建同步作业的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源商城、水位或失败作业不存在
    /// * `ConflictError` - 阶段、水位版本变化或已有运行中增量任务
    /// * `ValidationError` - 命令身份、理由或模式字段非法
    pub async fn trigger_sync_job(
        &self,
        command: TriggerMallSyncCommand,
        actor: &AuditActor,
    ) -> Result<MallSalesSyncJobView> {
        ensure_trigger_command(&command)?;
        let identity = mall_command_identity(
            actor.id(),
            "trigger-sync",
            command.source_system_id().as_ref(),
            command.idempotency_key(),
            &command,
        )?;
        let fingerprint = identity.fingerprint().to_string();
        let audit_id = identity.audit_id().to_string();
        if let Some(view) = self.replay_sync_trigger(&audit_id, &fingerprint).await? {
            return Ok(view);
        }

        self.ensure_expected_mapping_stage(
            command.source_system_id(),
            command.execution_stage(),
            &mut NoTransaction,
        )
        .await?;
        let now = Instant::now();
        let spec = self.trigger_job_spec(&command, actor, now).await?;
        if spec.job_type == MallSalesSyncJobType::Incremental
            && self
                .db
                .mall_sales_sync_jobs()
                .find_running_incremental_by_source(command.source_system_id(), &mut NoTransaction)
                .await?
                .is_some()
        {
            return Err(Error::ConflictError(
                "该来源商城已有运行中的增量任务，禁止并发推进水位".to_string(),
            ));
        }

        let job_id = identity.sync_job_id();
        let job = MallSalesSyncJob::new(
            MallSalesSyncJobId::new(job_id.clone()),
            MallSalesSyncJobData {
                source_system_id: command.source_system_id().clone(),
                job_type: spec.job_type,
                range_start: spec.range_start,
                range_end: spec.range_end,
                external_order_no: spec.external_order_no,
                trigger_source: spec.trigger_source,
                trigger_reason: spec.trigger_reason,
                triggered_by: spec.triggered_by,
                source_job_id: spec.source_job_id,
                started_at: now,
            },
        )?;
        let audit = actor.clone().resource_log_with_id(
            audit_id.clone(),
            "mall_sales_sync_job.trigger",
            "mall_sales_sync_job",
            job_id,
            Some(command_audit_message(&fingerprint)),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let job_for_tx = job.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_sales_sync_jobs().create(&job_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        match transaction_result {
            Ok(()) => Ok(job.into()),
            Err(error) => self
                .replay_sync_trigger(&audit_id, &fingerprint)
                .await?
                .ok_or(error),
        }
    }

    async fn trigger_job_spec(
        &self,
        command: &TriggerMallSyncCommand,
        actor: &AuditActor,
        now: Instant,
    ) -> Result<TriggerJobSpec> {
        match command {
            TriggerMallSyncCommand::Incremental {
                source_system_id,
                trigger_source,
                reason,
                base_cursor_version,
                ..
            } => {
                let cursor = self
                    .db
                    .mall_sales_sync_cursors()
                    .find_by_source(source_system_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| {
                        Error::BusinessLogicError("来源商城尚未形成安全水位，不能执行增量同步".to_string())
                    })?;
                if base_cursor_version.is_some_and(|expected| !cursor.has_version(expected)) {
                    return Err(Error::ConflictError(
                        "同步水位版本已变化，请刷新后重试".to_string(),
                    ));
                }
                let range = MallSyncTimeRange::incremental(
                    cursor.high_water_updated_at,
                    now,
                    MALL_SYNC_OVERLAP_SECONDS,
                )?;
                let (trigger_reason, triggered_by) =
                    trigger_actor_fields(*trigger_source, reason.as_deref(), actor.id())?;
                Ok(TriggerJobSpec {
                    job_type: MallSalesSyncJobType::Incremental,
                    range_start: Some(range.start),
                    range_end: Some(range.end),
                    external_order_no: None,
                    trigger_source: *trigger_source,
                    trigger_reason,
                    triggered_by,
                    source_job_id: None,
                })
            }
            TriggerMallSyncCommand::SingleOrder {
                trigger_source,
                external_order_no,
                reason,
                ..
            } => {
                if *trigger_source != MallSyncTriggerSource::Manual {
                    return Err(Error::ValidationError(
                        "按单号补拉只能由授权用户人工触发".to_string(),
                    ));
                }
                let (trigger_reason, triggered_by) =
                    trigger_actor_fields(*trigger_source, Some(reason), actor.id())?;
                Ok(TriggerJobSpec {
                    job_type: MallSalesSyncJobType::SingleOrderBackfill,
                    range_start: None,
                    range_end: None,
                    external_order_no: Some(required_trigger_text(external_order_no, "原来源销售单号")?),
                    trigger_source: *trigger_source,
                    trigger_reason,
                    triggered_by,
                    source_job_id: None,
                })
            }
            TriggerMallSyncCommand::RetryFailedJob {
                source_system_id,
                failed_job_id,
                reason,
                base_cursor_version,
                ..
            } => {
                let original = self
                    .db
                    .mall_sales_sync_jobs()
                    .find_by_id(failed_job_id.trim(), &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("待重试同步作业不存在".to_string()))?;
                if &original.source_system_id != source_system_id {
                    return Err(Error::ValidationError(
                        "失败作业不属于命令指定的来源商城".to_string(),
                    ));
                }
                if !original.is_retryable() {
                    return Err(Error::ConflictError(
                        "只有失败或部分失败的同步作业可以重试".to_string(),
                    ));
                }
                if let Some(expected) = base_cursor_version {
                    let cursor = self
                        .db
                        .mall_sales_sync_cursors()
                        .find_by_source(source_system_id, &mut NoTransaction)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源商城安全水位不存在".to_string()))?;
                    if !cursor.has_version(*expected) {
                        return Err(Error::ConflictError(
                            "同步水位版本已变化，请刷新后重试".to_string(),
                        ));
                    }
                }
                Ok(TriggerJobSpec {
                    job_type: original.job_type,
                    range_start: original.range_start,
                    range_end: original.range_end,
                    external_order_no: original.external_order_no,
                    trigger_source: MallSyncTriggerSource::Manual,
                    trigger_reason: Some(required_trigger_text(reason, "重试理由")?),
                    triggered_by: Some(actor.id().to_string()),
                    source_job_id: Some(MallSalesSyncJobId::new(original.base.id)),
                })
            }
            TriggerMallSyncCommand::Reconciliation {
                reason,
                reconciliation_boundary,
                ..
            } => Ok(TriggerJobSpec {
                job_type: MallSalesSyncJobType::MonthlyReconciliation,
                range_start: Some(reconciliation_boundary.as_of),
                range_end: Some(reconciliation_boundary.as_of),
                external_order_no: None,
                trigger_source: MallSyncTriggerSource::Manual,
                trigger_reason: Some(required_trigger_text(reason, "核对理由")?),
                triggered_by: Some(actor.id().to_string()),
                source_job_id: None,
            }),
        }
    }

    async fn replay_sync_trigger(
        &self,
        audit_id: &str,
        fingerprint: &str,
    ) -> Result<Option<MallSalesSyncJobView>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != "mall_sales_sync_job.trigger"
            || audit.resource_type != "mall_sales_sync_job"
            || audit_command_fingerprint(audit.message.as_deref().unwrap_or_default()) != Some(fingerprint)
        {
            return Err(Error::ConflictError("该同步触发幂等键已用于不同命令".to_string()));
        }
        let job_id = audit
            .resource_id
            .as_deref()
            .ok_or_else(|| Error::Internal("同步触发回执缺少作业身份".to_string()))?;
        let job = self
            .db
            .mall_sales_sync_jobs()
            .find_by_id(job_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("同步触发回执对应的作业不存在".to_string()))?;
        Ok(Some(job.into()))
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
                external_order_no: row.external_order_no,
                trigger_source: row.trigger_source,
                trigger_reason: row.trigger_reason,
                triggered_by: row.triggered_by,
                source_job_id: row.source_job_id.map(|id| id.to_string()),
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
        match job.completion_disposition(outcome) {
            SyncJobCompletionDisposition::Apply => {}
            SyncJobCompletionDisposition::AlreadyApplied => {
                tracing::info!(job_id = id, status = ?job.status, "作业已终态，按幂等返回");
                return Ok(job.into());
            }
            SyncJobCompletionDisposition::ConflictingTerminal => {
                return Err(Error::ConflictError("同步作业已终态，结果不一致".to_string()));
            }
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
        if item.status.is_terminal() {
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
        actor: &AuditActor,
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
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let task = self
                .db
                .master_mapping_tasks()
                .find_by_id(&row.id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::Internal("映射任务列表投影对应的领域对象不存在".to_string()))?;
            items.push(self.mapping_task_view(task, None, actor).await?);
        }

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询 actor-specific 映射任务详情。
    ///
    /// 若从正式队列携带 `work_item_id`，该任务必须精确关联路径中的映射任务；
    /// 不匹配时失败关闭，不回退到该对象的第一条任务。
    pub async fn mapping_task_detail(
        &self,
        id: &str,
        params: &MasterMappingTaskDetailParams,
        actor: &AuditActor,
    ) -> Result<MasterMappingTaskView> {
        let task = self
            .db
            .master_mapping_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("映射任务不存在".to_string()))?;
        self.mapping_task_view(task, params.work_item_id.as_deref(), actor)
            .await
    }

    async fn mapping_task_view(
        &self,
        task: MasterMappingTask,
        explicit_work_item_id: Option<&str>,
        actor: &AuditActor,
    ) -> Result<MasterMappingTaskView> {
        let snapshot = self
            .db
            .mall_sales_order_snapshots()
            .find_by_id(task.source_snapshot_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("映射任务引用的来源快照不存在".to_string()))?;
        let work_item = self
            .mapping_work_item_for_task(&task, explicit_work_item_id)
            .await?;
        let routing_configured = task.owner_role.is_some() && work_item.is_some();
        let eligible = work_item.is_some();
        let source = self
            .db
            .source_systems()
            .find_by_id(snapshot.source_system_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("映射任务引用的来源商城不存在".to_string()))?;
        let stage_active = source.system_type == SourceSystemType::Mall
            && source.stable.status == SourceSystemStatus::Active
            && source.mall_sync_stage == Some(MallSyncStage::FirstPhaseMallOwned);

        let candidate_targets = if eligible && stage_active {
            self.mapping_candidates(task.mapping_type, actor.id()).await?
        } else {
            Vec::new()
        };
        let (external_identity_map_id, current_targets, lineage_error) =
            match mapping_external_identity(&snapshot.normalized_snapshot, task.mapping_type) {
                Ok(external_id) => {
                    let (map_id, targets) = self
                        .mapping_lineage_view(&snapshot.source_system_id, task.mapping_type, &external_id)
                        .await?;
                    (map_id, targets, None)
                }
                Err(error) => (None, Vec::new(), Some(error.to_string())),
            };
        let latest_reapply = self
            .db
            .mall_snapshot_reapply_operations()
            .latest_reapply_for_task(&task.base.id, &mut NoTransaction)
            .await?
            .map(ReapplyOperationView::from);
        let resolution_history = self.mapping_resolution_history(&task).await?;
        let source_evidence = mapping_source_evidence(&snapshot, task.mapping_type);

        let mut allowed_actions = Vec::new();
        let mut action_blockers = Vec::new();
        let owns_open_task = work_item.as_ref().is_some_and(|item| {
            item.status == WorkItemStatus::Open && item.is_owned_by(actor.id()) && eligible
        });
        if !stage_active {
            for action in ["CONFIRM_TARGET", "REQUEST_SOURCE_FIX", "REAPPLY"] {
                action_blockers.push(mapping_blocker(
                    action,
                    "MALL_SYNC_ARCHIVED",
                    "来源商城未处于一期可写阶段，W17 仅保留历史查询",
                ));
            }
        } else {
            match task.status {
                MappingTaskStatus::Pending => {
                    if !routing_configured {
                        action_blockers.push(mapping_blocker(
                            "CONFIRM_TARGET",
                            "OWNER_ROUTING_MISSING",
                            "当前映射类型尚未形成唯一责任路由与正式任务",
                        ));
                        action_blockers.push(mapping_blocker(
                            "REQUEST_SOURCE_FIX",
                            "OWNER_ROUTING_MISSING",
                            "当前映射类型尚未形成唯一责任路由与正式任务",
                        ));
                    } else if owns_open_task {
                        allowed_actions.push("REQUEST_SOURCE_FIX".to_string());
                        if task.mapping_type.target_registration().is_none() {
                            action_blockers.push(mapping_blocker(
                                "CONFIRM_TARGET",
                                "MAPPING_TYPE_NOT_REGISTERED",
                                "该差异类型没有独立 ERP 规范目标模型，只能追加来源修复证据",
                            ));
                        } else if candidate_targets.is_empty() {
                            action_blockers.push(mapping_blocker(
                                "CONFIRM_TARGET",
                                "TARGET_CANDIDATE_EMPTY",
                                "当前责任与数据范围内没有可确认的有效 ERP 目标",
                            ));
                        } else if let Some(message) = lineage_error.as_deref() {
                            action_blockers.push(mapping_blocker(
                                "CONFIRM_TARGET",
                                "SOURCE_IDENTITY_INVALID",
                                message,
                            ));
                        } else {
                            allowed_actions.push("CONFIRM_TARGET".to_string());
                        }
                    } else {
                        action_blockers.push(mapping_blocker(
                            "CONFIRM_TARGET",
                            "RESPONSIBILITY_NOT_HELD",
                            "当前账号尚未取得该正式任务的个人责任",
                        ));
                        action_blockers.push(mapping_blocker(
                            "REQUEST_SOURCE_FIX",
                            "RESPONSIBILITY_NOT_HELD",
                            "当前账号尚未取得该正式任务的个人责任",
                        ));
                    }
                }
                MappingTaskStatus::Resolved
                    if eligible
                        && routing_configured
                        && snapshot.applied_sales_order_revision_id.is_some() =>
                {
                    allowed_actions.push("REAPPLY".to_string());
                }
                MappingTaskStatus::Resolved if eligible && routing_configured => {
                    action_blockers.push(mapping_blocker(
                        "REAPPLY",
                        "REAPPLY_EXECUTOR_UNAVAILABLE",
                        "当前环境尚未注册原快照归集执行器，禁止把排队或固定失败视为归集成功",
                    ));
                }
                MappingTaskStatus::Resolved => action_blockers.push(mapping_blocker(
                    "REAPPLY",
                    "RESPONSIBILITY_NOT_ELIGIBLE",
                    "当前账号不具备该映射责任角色资格",
                )),
                MappingTaskStatus::Unresolvable | MappingTaskStatus::Closed => {
                    action_blockers.push(mapping_blocker(
                        "REAPPLY",
                        "MAPPING_TASK_NOT_RESOLVED",
                        "只有已解决的映射任务可以重新归集",
                    ));
                }
            }
        }

        let projected_work_item = work_item.as_ref().map(|item| MappingTaskWorkItemView {
            work_item_id: item.base.id.clone(),
            task_version: item.base.version.to_string(),
            work_item_type: item.work_item_type,
            business_object_type: item.business_object_type.clone(),
            business_object_id: item.business_object_id.clone(),
            subject_version: item.subject_version.clone(),
            status: item.status,
            owner_user_id: item.owner_user_id.clone(),
        });
        let owner_role = routing_configured
            .then(|| work_item.as_ref().map(|item| item.owner_role.clone()))
            .flatten();
        let owner_user_id = routing_configured
            .then(|| work_item.as_ref().and_then(|item| item.owner_user_id.clone()))
            .flatten();
        let lock_version = task.base.version;
        Ok(MasterMappingTaskView {
            id: task.base.id,
            source_snapshot_id: task.source_snapshot_id.to_string(),
            mapping_type: task.mapping_type,
            status: task.status,
            owner_role,
            owner_user_id,
            resolution: task.resolution,
            resolved_at: task.resolved_at,
            version: lock_version,
            created_at: task.base.created_at,
            owner_routing_state: if routing_configured {
                OwnerRoutingState::Configured
            } else {
                OwnerRoutingState::Missing
            },
            work_item: routing_configured.then_some(projected_work_item).flatten(),
            source_evidence,
            candidate_targets,
            current_targets,
            external_identity_map_id,
            impact_summary: format!(
                "{}映射未完成将阻断正确客户、应收、收入或经营归属，来源捕获水位不回退",
                task.mapping_type.label()
            ),
            resolution_history,
            allowed_actions,
            action_blockers,
            reapply_operation: latest_reapply,
            lock_version,
        })
    }

    async fn mapping_work_item_for_task(
        &self,
        task: &MasterMappingTask,
        explicit_work_item_id: Option<&str>,
    ) -> Result<Option<WorkItem>> {
        if let Some(work_item_id) = explicit_work_item_id {
            let work_item_id = required_trigger_text(work_item_id, "正式任务ID")?;
            let item = self
                .db
                .work_items()
                .find_by_id(&work_item_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("映射正式任务不存在".to_string()))?;
            ensure_mapping_work_item_identity(&item, task)?;
            return Ok(Some(item));
        }
        let mut items = self
            .db
            .work_items()
            .list_for_master_mapping_task(&task.base.id, &mut NoTransaction)
            .await?;
        if items.len() > 1 {
            return Err(Error::Internal(
                "同一映射任务存在多个正式任务，责任事实不唯一".to_string(),
            ));
        }
        Ok(items.pop())
    }

    async fn mapping_lineage_view(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        mapping_type: MappingTaskType,
        external_id: &str,
    ) -> Result<(Option<String>, Vec<MappingCurrentTargetView>)> {
        let Some(registration) = mapping_type.target_registration() else {
            return Ok((None, Vec::new()));
        };
        let key = ExternalIdentityMap::external_id_key(external_id);
        let Some(mapping) = self
            .db
            .external_identity_maps()
            .find_by_identity(
                source_system_id,
                registration.object_type,
                &key,
                &mut NoTransaction,
            )
            .await?
        else {
            return Ok((None, Vec::new()));
        };
        let targets = self
            .db
            .external_identity_targets()
            .list_for_external_identity_map(&mapping.base.id, &mut NoTransaction)
            .await?;
        Ok((
            Some(mapping.base.id),
            targets
                .into_iter()
                .map(|target| MappingCurrentTargetView {
                    mapping_target_id: target.base.id,
                    object_type: target.internal_object_type.as_str().to_string(),
                    object_id: target.internal_object_id,
                    relation_role: target.relation_role,
                    valid_from: target.valid_from,
                    valid_to: target.valid_to,
                    status: target.status.as_str().to_string(),
                })
                .collect(),
        ))
    }

    async fn mapping_candidates(
        &self,
        mapping_type: MappingTaskType,
        actor_id: &str,
    ) -> Result<Vec<MappingCandidateTargetView>> {
        match mapping_type {
            MappingTaskType::Customer => self.customer_mapping_candidates(actor_id).await,
            MappingTaskType::Contract => self.contract_mapping_candidates(actor_id).await,
            MappingTaskType::SettlementEntity => self.settlement_mapping_candidates(actor_id).await,
            MappingTaskType::VoucherCategory => self.voucher_mapping_candidates().await,
            MappingTaskType::UniqueLineItem | MappingTaskType::AmountFormat => Ok(Vec::new()),
        }
    }

    async fn customer_mapping_candidates(&self, actor_id: &str) -> Result<Vec<MappingCandidateTargetView>> {
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        let mut seen = std::collections::HashSet::new();
        let mut candidates = Vec::new();
        for assignment in assignments {
            if !seen.insert(assignment.customer_id.to_string()) || candidates.len() >= 50 {
                continue;
            }
            let Some(customer) = self
                .db
                .customer_accounts()
                .find_by_id(assignment.customer_id.as_ref(), &mut NoTransaction)
                .await?
            else {
                continue;
            };
            if !customer.stable.status.is_active() {
                continue;
            }
            let Some(party) = self
                .db
                .parties()
                .find_by_id(customer.party_id.as_ref(), &mut NoTransaction)
                .await?
            else {
                continue;
            };
            let Some(revision_id) = party.stable.current_revision_id.clone() else {
                continue;
            };
            let Some(revision) = self
                .db
                .party_revisions()
                .find_by_id(&revision_id, &mut NoTransaction)
                .await?
            else {
                continue;
            };
            candidates.push(MappingCandidateTargetView {
                object_type: "CUSTOMER".to_string(),
                object_id: customer.base.id,
                stable_no: customer.customer_no,
                label: revision.legal_name,
                current_revision_id: revision_id,
                eligibility: "ELIGIBLE".to_string(),
                reason: "当前账号对该客户具有有效销售归属".to_string(),
            });
        }
        Ok(candidates)
    }

    async fn contract_mapping_candidates(&self, actor_id: &str) -> Result<Vec<MappingCandidateTargetView>> {
        let customer_ids = self.actor_customer_ids(actor_id).await?;
        let page = self
            .db
            .contracts()
            .search_contracts(
                &ContractFilter {
                    contract_no: None,
                    customer_id: None,
                    customer_ids: Some(customer_ids),
                    status: Some(ContractStatus::Effective),
                    page: 1,
                    page_size: 50,
                    sort_by: Some("created_at".to_string()),
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(page
            .items
            .into_iter()
            .filter_map(|contract| {
                contract
                    .current_revision_id
                    .map(|revision_id| MappingCandidateTargetView {
                        object_type: "CONTRACT".to_string(),
                        object_id: contract.id,
                        stable_no: contract.contract_no.clone(),
                        label: contract.contract_no,
                        current_revision_id: revision_id,
                        eligibility: "ELIGIBLE".to_string(),
                        reason: "合同生效且属于当前账号有效客户范围".to_string(),
                    })
            })
            .collect())
    }

    async fn settlement_mapping_candidates(&self, actor_id: &str) -> Result<Vec<MappingCandidateTargetView>> {
        let customer_ids = self.actor_customer_ids(actor_id).await?;
        let contracts = self
            .db
            .contracts()
            .search_contracts(
                &ContractFilter {
                    contract_no: None,
                    customer_id: None,
                    customer_ids: Some(customer_ids),
                    status: Some(ContractStatus::Effective),
                    page: 1,
                    page_size: 50,
                    sort_by: Some("created_at".to_string()),
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await?;
        let mut seen = std::collections::HashSet::new();
        let mut candidates = Vec::new();
        for contract in contracts.items {
            if !seen.insert(contract.settlement_party_id.clone()) {
                continue;
            }
            let Some(party) = self
                .db
                .parties()
                .find_by_id(&contract.settlement_party_id, &mut NoTransaction)
                .await?
            else {
                continue;
            };
            if !party.is_active() {
                continue;
            }
            let Some(revision_id) = party.stable.current_revision_id.clone() else {
                continue;
            };
            let Some(revision) = self
                .db
                .party_revisions()
                .find_by_id(&revision_id, &mut NoTransaction)
                .await?
            else {
                continue;
            };
            candidates.push(MappingCandidateTargetView {
                object_type: "SETTLEMENT_PARTY".to_string(),
                object_id: party.base.id,
                stable_no: party.party_no,
                label: revision.legal_name,
                current_revision_id: revision_id,
                eligibility: "ELIGIBLE".to_string(),
                reason: "结算主体来自当前账号有效客户合同".to_string(),
            });
        }
        Ok(candidates)
    }

    async fn voucher_mapping_candidates(&self) -> Result<Vec<MappingCandidateTargetView>> {
        let profiles = self
            .db
            .voucher_category_profile_revisions()
            .search_voucher_category_profile_revisions(
                &VoucherCategoryProfileRevisionFilter {
                    sku_id: None,
                    status: Some(EnableStatus::Active),
                    page: 1,
                    page_size: 100,
                    sort_by: Some("revision_no".to_string()),
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await?;
        let mut seen = std::collections::HashSet::new();
        let mut candidates = Vec::new();
        for profile in profiles.items {
            if !seen.insert(profile.sku_id.clone()) || candidates.len() >= 50 {
                continue;
            }
            let Some(sku) = self
                .db
                .skus()
                .find_by_id(&profile.sku_id, &mut NoTransaction)
                .await?
            else {
                continue;
            };
            if !sku.is_active() || !sku.listing_status.is_listed() {
                continue;
            }
            let Some(revision_id) = sku.stable.current_revision_id.clone() else {
                continue;
            };
            let Some(revision) = self
                .db
                .sku_revisions()
                .find_by_id(&revision_id, &mut NoTransaction)
                .await?
            else {
                continue;
            };
            candidates.push(MappingCandidateTargetView {
                object_type: "VOUCHER_CATEGORY".to_string(),
                object_id: sku.base.id,
                stable_no: sku.sku_no,
                label: revision.name,
                current_revision_id: revision_id,
                eligibility: "ELIGIBLE".to_string(),
                reason: "卡券类目扩展启用且 SKU 当前已启用上架".to_string(),
            });
        }
        Ok(candidates)
    }

    async fn actor_customer_ids(&self, actor_id: &str) -> Result<Vec<String>> {
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        let mut ids = assignments
            .into_iter()
            .map(|assignment| assignment.customer_id.to_string())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    async fn mapping_resolution_history(
        &self,
        task: &MasterMappingTask,
    ) -> Result<Vec<MappingResolutionHistoryView>> {
        let audits = self
            .db
            .audit_logs()
            .list_master_mapping_task_history(&task.base.id, &mut NoTransaction)
            .await?;
        Ok(audits
            .into_iter()
            .map(|audit| MappingResolutionHistoryView {
                action: audit.action,
                result: if audit.success { "SUCCEEDED" } else { "FAILED" }.to_string(),
                handled_by: audit.actor_id,
                handled_at: audit.base.created_at,
                evidence_reference: Some(audit.base.id),
            })
            .collect())
    }

    /// 创建映射任务及其唯一正式责任任务。
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
        let owner_user_id = req.owner_user_id.trim().to_string();
        let snapshot = self
            .db
            .mall_sales_order_snapshots()
            .find_by_id(req.source_snapshot_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商城销售单快照不存在".to_string()))?;
        self.ensure_mapping_stage(&snapshot.source_system_id, &mut NoTransaction)
            .await?;
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

        let owner_role = req.mapping_type.owner_role().map(str::to_string);
        let task = MasterMappingTask::new(
            MasterMappingTaskId::new(next_id()),
            MasterMappingTaskData {
                source_snapshot_id: req.source_snapshot_id,
                mapping_type: req.mapping_type,
                owner_role: owner_role.clone(),
                owner_user_id: owner_role.as_ref().map(|_| owner_user_id.clone()),
            },
        )?;
        let audit = actor.clone().resource_log(
            "master_mapping_task.create",
            "master_mapping_task",
            task.base.id.clone(),
        )?;

        let work_item = owner_role
            .map(|owner_role| mapping_work_item(&task, owner_role, &owner_user_id))
            .transpose()?;
        let work_item_id = work_item.as_ref().map(|item| item.base.id.clone());
        let db = self.db.clone();
        let client = db.client().clone();
        let task_for_tx = task.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.master_mapping_tasks().create(&task_for_tx, session).await?;
                    if let Some(work_item) = work_item.as_ref() {
                        db.work_items().create(work_item, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.mapping_task_view(task, work_item_id.as_deref(), actor).await
    }

    /// 按固定映射类型注册表确认规范目标并原子完成 W17 正式任务。
    ///
    /// # 参数
    /// * `id` - 映射任务 ID
    /// * `command` - 带三重版本与幂等键的强类型命令
    /// * `actor` - 当前认证操作人
    ///
    /// # 返回
    /// 返回映射谱系与正式任务完成结果。
    ///
    /// # 错误
    /// 责任、阶段、版本、目标或来源身份无法被服务端证明时失败关闭。
    pub async fn confirm_mapping_task(
        &self,
        id: &str,
        command: ConfirmMappingCommand,
        actor: &AuditActor,
    ) -> Result<ConfirmMappingResult> {
        command.validate()?;
        ensure_path_id(id, &command.decision.mapping_task_id)?;
        let expected_task_version = parse_positive_version(&command.expected_task_version, "待办版本")?;
        let identity = mall_command_identity(actor.id(), "confirm", id, &command.idempotency_key, &command)?;
        let fingerprint = identity.fingerprint().to_string();
        let audit_id = identity.audit_id().to_string();
        let (generated_map_id, target_id) = identity.mapping_lineage_ids();
        let map_id = command
            .decision
            .external_identity_map_id
            .clone()
            .unwrap_or(generated_map_id);
        if let Some(result) = self
            .replay_confirm(&audit_id, &fingerprint, &command, &map_id, &target_id)
            .await?
        {
            return Ok(result);
        }

        let transaction_result = self
            .confirm_mapping_transaction(ConfirmMappingTxInput {
                command: command.clone(),
                actor: actor.clone(),
                expected_task_version,
                fingerprint: fingerprint.clone(),
                audit_id: audit_id.clone(),
                map_id: map_id.clone(),
                target_id: target_id.clone(),
            })
            .await;
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => self
                .replay_confirm(&audit_id, &fingerprint, &command, &map_id, &target_id)
                .await?
                .ok_or(error),
        }
    }

    /// 请求来源修复并追加不可变审计证据，保持映射任务与正式任务开放。
    ///
    /// # 错误
    /// 责任、阶段、版本或证据清单无法通过校验时返回错误。
    pub async fn request_mapping_source_fix(
        &self,
        id: &str,
        command: RequestSourceFixCommand,
        actor: &AuditActor,
    ) -> Result<RequestSourceFixResult> {
        command.validate()?;
        ensure_path_id(id, &command.action.mapping_task_id)?;
        ensure_evidence_list(&command.action.requested_evidence)?;
        let expected_task_version = parse_positive_version(&command.expected_task_version, "待办版本")?;
        let identity = mall_command_identity(
            actor.id(),
            "request-source-fix",
            id,
            &command.idempotency_key,
            &command,
        )?;
        let fingerprint = identity.fingerprint().to_string();
        let audit_id = identity.audit_id().to_string();
        if let Some(result) = self.replay_source_fix(&audit_id, &fingerprint, &command).await? {
            return Ok(result);
        }

        let transaction_result = self
            .source_fix_transaction(
                command.clone(),
                actor.clone(),
                expected_task_version,
                fingerprint.clone(),
                audit_id.clone(),
            )
            .await;
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => self
                .replay_source_fix(&audit_id, &fingerprint, &command)
                .await?
                .ok_or(error),
        }
    }

    /// 创建可独立查询、幂等仲裁的重新归集操作。
    ///
    /// 已存在正式应用结果时返回可验证销售版本与应收引用；当前没有归集执行器
    /// 时仍形成明确 `FAILED` 操作事实，禁止以 HTTP 固定失败或前端伪成功替代。
    ///
    /// # 错误
    /// 请求非法、任务未解决、阶段不匹配或同幂等键异参时返回对应错误。
    pub async fn reapply_mapping_task(
        &self,
        id: &str,
        command: ReapplyMallSnapshotCommand,
        actor: &AuditActor,
    ) -> Result<GovernanceActionResult> {
        command.validate()?;
        ensure_path_id(id, &command.mapping_task_id)?;
        required_trigger_text(&command.operation_id, "重新归集操作ID")?;
        required_trigger_text(&command.idempotency_key, "幂等键")?;
        let identity = mall_command_identity(actor.id(), "reapply", id, &command.idempotency_key, &command)?;
        let idempotency_key_hash = identity.idempotency_key_hash().to_string();
        let fingerprint = identity.fingerprint().to_string();
        if let Some(operation) = self
            .replay_reapply_operation(id, &command.operation_id, &idempotency_key_hash, &fingerprint)
            .await?
        {
            return Ok(operation.into());
        }
        let task = self
            .db
            .master_mapping_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("映射任务不存在".to_string()))?;
        if !task.has_version(command.expected_mapping_version) {
            return Err(Error::ConflictError(
                "映射任务版本已变化，请刷新后重试".to_string(),
            ));
        }
        if !task.can_reapply() {
            return Err(Error::ConflictError(
                "只有已解决的映射任务可以重新归集".to_string(),
            ));
        }
        ensure_path_id(
            task.source_snapshot_id.as_ref(),
            command.source_snapshot_id.as_ref(),
        )?;
        let snapshot = self
            .db
            .mall_sales_order_snapshots()
            .find_by_id(command.source_snapshot_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商城销售单快照不存在".to_string()))?;
        self.ensure_expected_mapping_stage(
            &snapshot.source_system_id,
            command.execution_stage,
            &mut NoTransaction,
        )
        .await?;
        if snapshot.applied_sales_order_revision_id.is_none() {
            return Err(Error::BusinessLogicError(
                "当前环境尚未注册原快照归集执行器，禁止创建必然失败的重新归集操作".to_string(),
            ));
        }
        let now = Instant::now();
        let mut operation = MallSnapshotReapplyOperation::new(
            command.operation_id.clone(),
            MallSnapshotReapplyOperationData {
                mapping_task_id: MasterMappingTaskId::new(task.base.id.clone()),
                source_snapshot_id: command.source_snapshot_id.clone(),
                idempotency_key_hash: idempotency_key_hash.clone(),
                command_fingerprint: fingerprint.clone(),
                requested_by: actor.id().to_string(),
                requested_at: now,
            },
        )?;
        if let Some(revision_id) = snapshot.applied_sales_order_revision_id.clone() {
            let revision = self
                .db
                .sales_order_revisions()
                .find_by_id(revision_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::Internal("快照引用的销售版本不存在".to_string()))?;
            let receivable = self
                .db
                .receivable_accounts()
                .find_by_source_sales_order_revision(&revision_id, &mut NoTransaction)
                .await?;
            if let Some(receivable) = receivable {
                operation.succeed(
                    revision.sales_order_id,
                    revision_id,
                    Some(receivable.base.id),
                    now,
                )?;
            } else {
                operation.fail(
                    "RECEIVABLE_RESULT_MISSING".to_string(),
                    "销售版本已形成，但尚无可验证的适用应收结果".to_string(),
                    now,
                )?;
            }
        }
        let audit = actor.clone().resource_log_with_id(
            identity.audit_id().to_string(),
            "master_mapping_task.reapply",
            W17_OBJECT_TYPE,
            task.base.id,
            Some(command_audit_message(&fingerprint)),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let operation_for_tx = operation.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_snapshot_reapply_operations()
                        .create(&operation_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        match transaction_result {
            Ok(()) => Ok(operation.into()),
            Err(error) => self
                .replay_reapply_operation(id, &command.operation_id, &idempotency_key_hash, &fingerprint)
                .await?
                .map(GovernanceActionResult::from)
                .ok_or(error),
        }
    }

    /// 按 task + operation ID 查询重新归集最终状态。
    pub async fn reapply_operation_detail(
        &self,
        mapping_task_id: &str,
        operation_id: &str,
    ) -> Result<ReapplyOperationView> {
        let operation = self
            .db
            .mall_snapshot_reapply_operations()
            .find_by_id(operation_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("重新归集操作不存在".to_string()))?;
        if operation.mapping_task_id.as_ref() != mapping_task_id {
            return Err(Error::ValidationError(
                "重新归集操作ID与路径映射任务不一致".to_string(),
            ));
        }
        Ok(operation.into())
    }

    async fn replay_reapply_operation(
        &self,
        mapping_task_id: &str,
        operation_id: &str,
        idempotency_key_hash: &str,
        fingerprint: &str,
    ) -> Result<Option<MallSnapshotReapplyOperation>> {
        if let Some(operation) = self
            .db
            .mall_snapshot_reapply_operations()
            .find_reapply_by_idempotency(mapping_task_id, idempotency_key_hash, &mut NoTransaction)
            .await?
        {
            ensure_reapply_receipt(
                &operation,
                mapping_task_id,
                operation_id,
                idempotency_key_hash,
                fingerprint,
            )?;
            return Ok(Some(operation));
        }
        let Some(operation) = self
            .db
            .mall_snapshot_reapply_operations()
            .find_by_id(operation_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_reapply_receipt(
            &operation,
            mapping_task_id,
            operation_id,
            idempotency_key_hash,
            fingerprint,
        )?;
        Ok(Some(operation))
    }

    /// 在同一事务内确认映射并写入谱系。
    ///
    /// # 用途
    /// 重验任务上下文后写入谱系、完成任务并登记审计。
    ///
    /// # 参数
    /// * `input` - 命令、审计与谱系标识
    ///
    /// # 返回
    /// 返回确认结果。
    ///
    /// # 错误
    /// 版本冲突、谱系变化或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 必须与任务完成同一事务。
    async fn confirm_mapping_transaction(
        &self,
        input: ConfirmMappingTxInput,
    ) -> Result<ConfirmMappingResult> {
        let ConfirmMappingTxInput {
            command,
            actor,
            expected_task_version,
            fingerprint,
            audit_id,
            map_id,
            target_id,
        } = input;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut context = load_mapping_context(
                        &db,
                        &command.decision.mapping_task_id,
                        &command.work_item_id,
                        session,
                    )
                    .await?;
                    validate_mapping_context(
                        &db,
                        &context,
                        &command,
                        expected_task_version,
                        actor.id(),
                        session,
                    )
                    .await?;
                    ensure_mapping_resolution_shape(context.task.mapping_type, &command)?;
                    ensure_mapping_target_access(
                        &db,
                        context.task.mapping_type,
                        &command.decision.resolution.object_id,
                        actor.id(),
                        session,
                    )
                    .await?;
                    let external_id = mapping_external_identity(
                        &context.snapshot.normalized_snapshot,
                        context.task.mapping_type,
                    )?;
                    let now = Instant::now();
                    let mut lineage = prepare_mapping_lineage(
                        &db,
                        MappingLineagePrep {
                            context: &context,
                            command: &command,
                            actor_id: actor.id(),
                            external_id: &external_id,
                            map_id: &map_id,
                            target_id: &target_id,
                            now,
                        },
                        session,
                    )
                    .await?;
                    context.task.resolve(
                        format!("{}映射已由责任人确认", context.task.mapping_type.label()),
                        now,
                    )?;
                    context.work_item.complete_by_domain_command(actor.id(), now)?;
                    if lineage.is_new_mapping {
                        db.source_registry()
                            .create_external_identity_link(&lineage.mapping, &lineage.target, session)
                            .await?;
                    } else {
                        for target in &mut lineage.expired_targets {
                            db.external_identity_targets().update(target, session).await?;
                        }
                        db.external_identity_maps()
                            .update(&mut lineage.mapping, session)
                            .await?;
                        db.external_identity_targets()
                            .create(&lineage.target, session)
                            .await?;
                    }
                    db.master_mapping_tasks()
                        .update(&mut context.task, session)
                        .await?;
                    db.work_items().update(&mut context.work_item, session).await?;
                    let audit = actor.clone().resource_log_with_id(
                        audit_id,
                        "master_mapping_task.confirm",
                        W17_OBJECT_TYPE,
                        context.task.base.id.clone(),
                        Some(command_audit_message(&fingerprint)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(confirm_result(
                        &context,
                        map_id,
                        target_id,
                        now,
                        command.decision.execution_stage,
                    ))
                })
            })
            .await
    }

    async fn source_fix_transaction(
        &self,
        command: RequestSourceFixCommand,
        actor: AuditActor,
        expected_task_version: u64,
        fingerprint: String,
        audit_id: String,
    ) -> Result<RequestSourceFixResult> {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut context = load_mapping_context(
                        &db,
                        &command.action.mapping_task_id,
                        &command.work_item_id,
                        session,
                    )
                    .await?;
                    validate_source_fix_context(
                        &db,
                        &context,
                        &command,
                        expected_task_version,
                        actor.id(),
                        session,
                    )
                    .await?;
                    let now = Instant::now();
                    context.work_item.record_activity(actor.id(), now)?;
                    db.work_items().update(&mut context.work_item, session).await?;
                    let message = format!(
                        "{};task_version={}",
                        command_audit_message(&fingerprint),
                        context.work_item.base.version
                    );
                    let audit = actor.clone().resource_log_with_id(
                        audit_id.clone(),
                        "master_mapping_task.request_source_fix",
                        W17_OBJECT_TYPE,
                        context.task.base.id.clone(),
                        Some(message),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(source_fix_result(&context, audit_id, now))
                })
            })
            .await
    }

    async fn replay_confirm(
        &self,
        audit_id: &str,
        fingerprint: &str,
        command: &ConfirmMappingCommand,
        map_id: &str,
        target_id: &str,
    ) -> Result<Option<ConfirmMappingResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_command_receipt(
            &audit,
            "master_mapping_task.confirm",
            &command.decision.mapping_task_id,
            fingerprint,
        )?;
        let context = load_mapping_context(
            &self.db,
            &command.decision.mapping_task_id,
            &command.work_item_id,
            &mut NoTransaction,
        )
        .await?;
        if !context.task.can_reapply()
            || context.work_item.status != WorkItemStatus::Completed
            || self
                .db
                .external_identity_maps()
                .find_by_id(map_id, &mut NoTransaction)
                .await?
                .is_none()
            || self
                .db
                .external_identity_targets()
                .find_by_id(target_id, &mut NoTransaction)
                .await?
                .is_none()
        {
            return Err(Error::Internal("W17 幂等回执对应的业务结果不完整".to_string()));
        }
        Ok(Some(confirm_result(
            &context,
            map_id.to_string(),
            target_id.to_string(),
            Instant::from_unix_secs(audit.base.created_at as i64),
            command.decision.execution_stage,
        )))
    }

    async fn replay_source_fix(
        &self,
        audit_id: &str,
        fingerprint: &str,
        command: &RequestSourceFixCommand,
    ) -> Result<Option<RequestSourceFixResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        ensure_command_receipt(
            &audit,
            "master_mapping_task.request_source_fix",
            &command.action.mapping_task_id,
            fingerprint,
        )?;
        let context = load_mapping_context(
            &self.db,
            &command.action.mapping_task_id,
            &command.work_item_id,
            &mut NoTransaction,
        )
        .await?;
        let task_version = receipt_task_version(audit.message.as_deref())?;
        Ok(Some(RequestSourceFixResult {
            work_item_id: context.work_item.base.id,
            work_item_status: WorkItemStatus::Open,
            task_version: task_version.to_string(),
            mapping_task_id: context.task.base.id,
            mapping_task_status: MappingTaskStatus::Pending,
            mapping_evidence_entry_id: audit.base.id,
            recorded_at: Instant::from_unix_secs(audit.base.created_at as i64),
        }))
    }

    /// 从权威来源配置读取 W17 当前阶段；缺失、停用或非一期均失败关闭。
    async fn ensure_mapping_stage(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_source_mapping_stage(&self.db, source_system_id, None, executor).await
    }

    /// 同时校验客户端冻结阶段与服务端当前阶段，禁止陈旧阶段写入。
    async fn ensure_expected_mapping_stage(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        expected_stage: MallSyncStage,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_source_mapping_stage(&self.db, source_system_id, Some(expected_stage), executor).await
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

async fn load_mapping_context(
    db: &Database,
    mapping_task_id: &str,
    work_item_id: &str,
    executor: &mut dyn Executor,
) -> Result<MappingCommandContext> {
    let task = db
        .master_mapping_tasks()
        .find_by_id(mapping_task_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("映射任务不存在".to_string()))?;
    let snapshot = db
        .mall_sales_order_snapshots()
        .find_by_id(task.source_snapshot_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("商城销售单快照不存在".to_string()))?;
    let work_item = db
        .work_items()
        .find_by_id(work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("映射正式任务不存在".to_string()))?;
    Ok(MappingCommandContext {
        task,
        snapshot,
        work_item,
    })
}

async fn validate_mapping_context(
    db: &Database,
    context: &MappingCommandContext,
    command: &ConfirmMappingCommand,
    expected_task_version: u64,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_mapping_task_contract(
        context,
        &command.decision.source_snapshot_id,
        command.decision.expected_mapping_task_version,
        &command.expected_subject_version,
        expected_task_version,
        actor_id,
    )?;
    if context.task.mapping_type.target_registration().is_none() {
        return Err(Error::BusinessLogicError(
            "当前映射类型未注册独立 ERP 规范目标模型，确认命令已失败关闭".to_string(),
        ));
    }
    ensure_source_mapping_stage(
        db,
        &context.snapshot.source_system_id,
        Some(command.decision.execution_stage),
        executor,
    )
    .await?;
    ensure_mapping_actor_eligible(db, context, actor_id, executor).await
}

async fn validate_source_fix_context(
    db: &Database,
    context: &MappingCommandContext,
    command: &RequestSourceFixCommand,
    expected_task_version: u64,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_mapping_task_contract(
        context,
        &command.action.source_snapshot_id,
        command.action.expected_mapping_task_version,
        &command.expected_subject_version,
        expected_task_version,
        actor_id,
    )?;
    ensure_source_mapping_stage(db, &context.snapshot.source_system_id, None, executor).await?;
    ensure_mapping_actor_eligible(db, context, actor_id, executor).await
}

fn ensure_mapping_task_contract(
    context: &MappingCommandContext,
    source_snapshot_id: &MallSalesOrderSnapshotId,
    expected_mapping_version: u64,
    expected_subject_version: &str,
    expected_task_version: u64,
    actor_id: &str,
) -> Result<()> {
    ensure_path_id(
        context.task.source_snapshot_id.as_ref(),
        source_snapshot_id.as_ref(),
    )?;
    if !context.task.has_version(expected_mapping_version) {
        return Err(Error::ConflictError(
            "映射任务版本已变化，请刷新后重试".to_string(),
        ));
    }
    ensure_version(context.work_item.base.version, expected_task_version, "待办")?;
    if !context.task.is_pending() || context.work_item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError(
            "映射任务或正式任务已离开开放状态".to_string(),
        ));
    }
    let owner_role = context
        .task
        .owner_role
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("映射任务没有唯一责任角色".to_string()))?;
    if context.work_item.work_item_type != WorkItemType::BusinessException
        || context.work_item.business_object_type != W17_OBJECT_TYPE
        || context.work_item.business_object_id != context.task.base.id
        || context.work_item.owner_role != owner_role
        || context.work_item.owner_organization_id != W17_OWNER_ORGANIZATION
        || !context.work_item.is_owned_by(actor_id)
        || context
            .task
            .owner_user_id
            .as_deref()
            .is_some_and(|owner| owner != actor_id)
    {
        return Err(Error::Forbidden("当前账号不是该映射任务的有效责任人".to_string()));
    }
    let expected_subject_version = expected_subject_version.trim();
    if expected_subject_version != context.work_item.subject_version
        || context.work_item.subject_version != context.task.subject_version()
    {
        return Err(Error::ConflictError(
            "映射对象版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

fn ensure_mapping_work_item_identity(item: &WorkItem, task: &MasterMappingTask) -> Result<()> {
    if item.work_item_type != WorkItemType::BusinessException
        || item.business_object_type != W17_OBJECT_TYPE
        || item.business_object_id != task.base.id
        || task.owner_role.as_deref() != Some(item.owner_role.as_str())
    {
        return Err(Error::ValidationError(
            "正式任务ID与映射任务或责任路由不一致".to_string(),
        ));
    }
    Ok(())
}

fn mapping_blocker(action: &str, code: &str, message: &str) -> MappingActionBlockerView {
    MappingActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn mapping_source_evidence(
    snapshot: &MallSalesOrderSnapshot,
    mapping_type: MappingTaskType,
) -> Vec<MappingSourceEvidenceView> {
    let mut evidence = vec![
        MappingSourceEvidenceView {
            field: "external_order_no".to_string(),
            label: "商城销售单号".to_string(),
            value: snapshot.external_order_no.clone(),
            sensitive: false,
        },
        MappingSourceEvidenceView {
            field: "source_status_code".to_string(),
            label: "来源状态码".to_string(),
            value: snapshot.source_status_code.clone(),
            sensitive: false,
        },
        MappingSourceEvidenceView {
            field: "source_updated_at".to_string(),
            label: "来源更新时间".to_string(),
            value: snapshot.source_updated_at.unix_secs().to_string(),
            sensitive: false,
        },
    ];
    let Some(registration) = mapping_type.target_registration() else {
        return evidence;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&snapshot.normalized_snapshot) else {
        return evidence;
    };
    let Some(object) = value.as_object() else {
        return evidence;
    };
    for field in registration.source_identity_fields {
        let Some(value) = object.get(*field) else {
            continue;
        };
        let Ok(value) = mapping_source_identity(value) else {
            continue;
        };
        evidence.push(MappingSourceEvidenceView {
            field: (*field).to_string(),
            label: MappingTaskType::source_identity_label(field).to_string(),
            value: value.into_string(),
            sensitive: false,
        });
    }
    evidence
}

async fn ensure_mapping_actor_eligible(
    db: &Database,
    context: &MappingCommandContext,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, context, actor_id, executor);
    Ok(())
}

async fn ensure_source_mapping_stage(
    db: &Database,
    source_system_id: &entities::ids::SourceSystemId,
    expected_stage: Option<MallSyncStage>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let source = db
        .source_systems()
        .find_by_id(source_system_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound(SOURCE_SYSTEM_NOT_FOUND_MESSAGE.to_string()))?;
    if source.system_type != SourceSystemType::Mall || source.stable.status != SourceSystemStatus::Active {
        return Err(Error::BusinessLogicError("来源商城不存在或已停用".to_string()));
    }
    let Some(current_stage) = source.mall_sync_stage else {
        return Err(Error::BusinessLogicError(
            "来源商城未配置同步阶段，写入已失败关闭".to_string(),
        ));
    };
    if expected_stage.is_some_and(|expected| expected != current_stage) {
        return Err(Error::ConflictError(
            "商城同步阶段已变化，请刷新后重试".to_string(),
        ));
    }
    if current_stage != MallSyncStage::FirstPhaseMallOwned {
        return Err(Error::BusinessLogicError(
            "当前来源未处于一期商城主导阶段，映射写入已失败关闭".to_string(),
        ));
    }
    Ok(())
}

async fn ensure_mapping_target_access(
    db: &Database,
    mapping_type: MappingTaskType,
    target_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let assignments = db
        .customer_assignments()
        .find_active_assignments_for_user(actor_id, BusinessDate::today(), executor)
        .await?;
    let customer_ids = assignments
        .iter()
        .map(|assignment| assignment.customer_id.to_string())
        .collect::<Vec<_>>();
    match mapping_type {
        MappingTaskType::Customer => {
            let customer = db
                .customer_accounts()
                .find_by_id(target_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("目标客户不存在".to_string()))?;
            if !customer.stable.status.is_active() {
                return Err(Error::BusinessLogicError("目标客户已停用".to_string()));
            }
            if !customer_ids.iter().any(|id| id == target_id) {
                return Err(Error::Forbidden("当前账号不具备目标客户参与权".to_string()));
            }
            let party = db
                .parties()
                .find_by_id(customer.party_id.as_ref(), executor)
                .await?
                .ok_or_else(|| Error::NotFound("目标客户主体不存在".to_string()))?;
            if !party.is_active() || party.stable.current_revision_id.is_none() {
                return Err(Error::BusinessLogicError(
                    "目标客户主体不可用或没有当前正式修订".to_string(),
                ));
            }
        }
        MappingTaskType::Contract => {
            let contract = db
                .contracts()
                .find_by_id(target_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("目标合同不存在".to_string()))?;
            if contract.stable.status != ContractStatus::Effective
                || contract.stable.current_revision_id.is_none()
            {
                return Err(Error::BusinessLogicError(
                    "目标合同必须处于生效状态并具有当前正式修订".to_string(),
                ));
            }
            if !customer_ids.iter().any(|id| id == contract.customer_id.as_ref()) {
                return Err(Error::Forbidden("目标合同不属于当前账号客户范围".to_string()));
            }
        }
        MappingTaskType::SettlementEntity => {
            let party = db
                .parties()
                .find_by_id(target_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("目标结算主体不存在".to_string()))?;
            if !party.is_active() || party.stable.current_revision_id.is_none() {
                return Err(Error::BusinessLogicError(
                    "目标结算主体不可用或没有当前正式修订".to_string(),
                ));
            }
            let contract = db
                .contracts()
                .find_effective_for_settlement_party(&customer_ids, target_id, executor)
                .await?;
            if contract.is_none() {
                return Err(Error::Forbidden(
                    "目标结算主体不属于当前账号有效客户合同".to_string(),
                ));
            }
        }
        MappingTaskType::VoucherCategory => {
            let sku = db
                .skus()
                .find_by_id(target_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("目标卡券类目 SKU 不存在".to_string()))?;
            if !sku.is_active() || !sku.listing_status.is_listed() || sku.stable.current_revision_id.is_none()
            {
                return Err(Error::BusinessLogicError(
                    "目标卡券类目 SKU 必须启用、上架并具有当前正式修订".to_string(),
                ));
            }
            let active_profile = db
                .voucher_category_profile_revisions()
                .find_active_by_sku(target_id, executor)
                .await?;
            if active_profile.is_none() {
                return Err(Error::BusinessLogicError(
                    "目标 SKU 没有启用的卡券类目扩展".to_string(),
                ));
            }
        }
        MappingTaskType::UniqueLineItem | MappingTaskType::AmountFormat => {
            return Err(Error::BusinessLogicError(
                "该差异类型没有独立 ERP 规范目标".to_string(),
            ));
        }
    }
    Ok(())
}

/// 将规范化快照 JSON 适配为领域来源身份候选值。
///
/// # 参数
/// * `snapshot` - 规范化商城快照 JSON
/// * `mapping_type` - 当前映射差异类型
///
/// # 返回
/// 返回由领域注册表判定的唯一来源身份。
///
/// # 错误
/// 快照不是 JSON 对象、来源字段类型非法，或领域身份规则不满足时返回错误。
fn mapping_external_identity(snapshot: &str, mapping_type: MappingTaskType) -> Result<String> {
    let registration = mapping_type
        .target_registration()
        .ok_or_else(|| Error::BusinessLogicError("该差异类型没有可注册的外部规范身份".to_string()))?;
    let value: serde_json::Value = serde_json::from_str(snapshot)
        .map_err(|_| Error::BusinessLogicError("规范化快照不是可验证的 JSON 对象".to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| Error::BusinessLogicError("规范化快照不是可验证的 JSON 对象".to_string()))?;
    let candidates = registration
        .source_identity_fields
        .iter()
        .filter_map(|field| object.get(*field))
        .map(mapping_source_identity)
        .collect::<Result<Vec<_>>>()?;
    Ok(mapping_type.external_identity(&candidates)?.into_string())
}

/// 将 JSON 标量适配为来源身份值对象。
///
/// # 参数
/// * `value` - 注册来源字段对应的 JSON 值
///
/// # 返回
/// 返回规范化来源身份候选值。
///
/// # 错误
/// 值不是字符串或整数，或字符串为空时返回错误。
fn mapping_source_identity(value: &serde_json::Value) -> Result<MappingSourceIdentity> {
    let value = match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) if value.is_i64() || value.is_u64() => value.to_string(),
        _ => {
            return Err(Error::BusinessLogicError(
                "快照来源标识必须是字符串或整数".to_string(),
            ));
        }
    };
    MappingSourceIdentity::new(value).map_err(Into::into)
}

fn ensure_mapping_resolution_shape(
    mapping_type: MappingTaskType,
    command: &ConfirmMappingCommand,
) -> Result<()> {
    let registration = mapping_type
        .target_registration()
        .ok_or_else(|| Error::BusinessLogicError("该差异类型没有独立 ERP 规范目标".to_string()))?;
    if mapping_type.accepts_target(
        &command.decision.resolution.object_type,
        command.decision.resolution.relation_role,
    ) {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "{}映射只接受 {} + {} 规范目标",
        mapping_type.label(),
        registration.command_object_type,
        registration.relation_role.as_str()
    )))
}

/// 映射谱系准备所需的命令、上下文与标识。
///
/// # 用途
/// 将确认命令、任务上下文与谱系主键打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 命令引用的谱系 ID 必须与当前来源身份一致。
struct MappingLineagePrep<'a> {
    /// 已加载的映射任务上下文。
    context: &'a MappingCommandContext,
    /// 确认映射命令。
    command: &'a ConfirmMappingCommand,
    /// 操作人 ID。
    actor_id: &'a str,
    /// 来源外部身份。
    external_id: &'a str,
    /// 谱系主键。
    map_id: &'a str,
    /// 目标主键。
    target_id: &'a str,
    /// 调用方时间。
    now: Instant,
}

/// 按命令准备外部身份谱系写入。
///
/// # 用途
/// 复用或新建谱系并准备过期目标与新目标。
///
/// # 参数
/// * `db` - 数据库
/// * `prep` - 命令、上下文与标识
/// * `executor` - 事务执行器
///
/// # 返回
/// 返回待写入的谱系、目标与过期目标。
///
/// # 错误
/// 谱系冲突、标识非法或仓储失败时返回错误。
///
/// # 关键业务约束
/// 已有谱系必须匹配命令引用；新谱系不得携带陈旧谱系 ID。
async fn prepare_mapping_lineage(
    db: &Database,
    prep: MappingLineagePrep<'_>,
    executor: &mut dyn Executor,
) -> Result<MappingLineageWrite> {
    let MappingLineagePrep {
        context,
        command,
        actor_id,
        external_id,
        map_id,
        target_id,
        now,
    } = prep;
    let registration = context
        .task
        .mapping_type
        .target_registration()
        .ok_or_else(|| Error::BusinessLogicError("该差异类型没有独立 ERP 规范目标".to_string()))?;
    let recorded_at = u64::try_from(now.unix_secs())
        .map_err(|_| Error::Internal("当前时间无法形成谱系时间".to_string()))?;
    let key = ExternalIdentityMap::external_id_key(external_id);
    let existing = db
        .external_identity_maps()
        .find_by_identity(
            &context.snapshot.source_system_id,
            registration.object_type,
            &key,
            executor,
        )
        .await?;
    let (mapping, mut expired_targets, is_new_mapping) = match existing {
        Some(mut mapping) => {
            if command.decision.external_identity_map_id.as_deref() != Some(mapping.base.id.as_str())
                || mapping.base.id != map_id
            {
                return Err(Error::ConflictError(
                    "当前来源身份谱系已变化，请刷新后重试".to_string(),
                ));
            }
            mapping.confirm_mapping(recorded_at, actor_id.to_string())?;
            let targets = db
                .external_identity_targets()
                .list_active_for_external_identity_map(&mapping.base.id, executor)
                .await?;
            (mapping, targets, false)
        }
        None => {
            if command.decision.external_identity_map_id.is_some() {
                return Err(Error::ConflictError(
                    "命令引用的外部身份谱系不存在，请刷新后重试".to_string(),
                ));
            }
            (
                ExternalIdentityMap::new(
                    ExternalIdentityMapId::new(map_id),
                    ExternalIdentityMapData {
                        source_system_id: context.snapshot.source_system_id.clone(),
                        object_type: registration.object_type,
                        external_id: external_id.to_string(),
                        mapping_status: MappingStatus::Mapped,
                        mapped_at: Some(recorded_at),
                        mapped_by: Some(actor_id.to_string()),
                    },
                )?,
                Vec::new(),
                true,
            )
        }
    };
    for target in &mut expired_targets {
        target.expire(recorded_at)?;
    }
    let target = ExternalIdentityTarget::new(
        ExternalIdentityTargetId::new(target_id),
        ExternalIdentityTargetData {
            external_identity_map_id: ExternalIdentityMapId::new(mapping.base.id.clone()),
            internal_object_type: registration.object_type,
            internal_object_id: command.decision.resolution.object_id.trim().to_string(),
            relation_role: registration.relation_role,
            valid_from: recorded_at,
            valid_to: None,
            status: TargetStatus::Active,
            approved_at: Some(recorded_at),
            approved_by: Some(actor_id.to_string()),
        },
    )?;
    Ok(MappingLineageWrite {
        mapping,
        target,
        expired_targets,
        is_new_mapping,
    })
}

fn confirm_result(
    context: &MappingCommandContext,
    map_id: String,
    target_id: String,
    recorded_at: Instant,
    execution_stage: MallSyncStage,
) -> ConfirmMappingResult {
    ConfirmMappingResult {
        work_item_id: context.work_item.base.id.clone(),
        work_item_status: context.work_item.status,
        business_result: dto::ConfirmMappingBusinessResult {
            mapping_task_id: context.task.base.id.clone(),
            mapping_task_status: context.task.status,
            external_identity_map_id: map_id,
            mapping_target_id: target_id,
            recorded_at,
            execution_stage,
        },
    }
}

fn source_fix_result(
    context: &MappingCommandContext,
    evidence_id: String,
    recorded_at: Instant,
) -> RequestSourceFixResult {
    RequestSourceFixResult {
        work_item_id: context.work_item.base.id.clone(),
        work_item_status: context.work_item.status,
        task_version: context.work_item.base.version.to_string(),
        mapping_task_id: context.task.base.id.clone(),
        mapping_task_status: context.task.status,
        mapping_evidence_entry_id: evidence_id,
        recorded_at,
    }
}

fn ensure_path_id(path_id: &str, body_id: &str) -> Result<()> {
    if path_id.trim() == body_id.trim() && !path_id.trim().is_empty() {
        return Ok(());
    }
    Err(Error::ValidationError("路径ID与命令对象ID不一致".to_string()))
}

fn ensure_version(actual: u64, expected: u64, object: &str) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{object}版本已变化，请刷新后重试")))
}

fn ensure_trigger_command(command: &TriggerMallSyncCommand) -> Result<()> {
    required_trigger_text(command.idempotency_key(), "幂等键")?;
    if command.execution_stage() != MallSyncStage::FirstPhaseMallOwned {
        return Err(Error::BusinessLogicError(
            "W17 只接受一期商城主导阶段的执行命令".to_string(),
        ));
    }
    Ok(())
}

fn trigger_actor_fields(
    trigger_source: MallSyncTriggerSource,
    reason: Option<&str>,
    actor_id: &str,
) -> Result<(Option<String>, Option<String>)> {
    match trigger_source {
        MallSyncTriggerSource::Scheduled if reason.is_none() => Ok((None, None)),
        MallSyncTriggerSource::Scheduled => {
            Err(Error::ValidationError("系统定时增量不得携带人工理由".to_string()))
        }
        MallSyncTriggerSource::Manual => Ok((
            Some(required_trigger_text(reason.unwrap_or_default(), "人工触发理由")?),
            Some(actor_id.to_string()),
        )),
    }
}

fn required_trigger_text(value: &str, label: &str) -> Result<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(Error::ValidationError(format!("{label}不能为空")));
    }
    Ok(normalized.to_string())
}

fn parse_positive_version(value: &str, label: &str) -> Result<u64> {
    let parsed = value
        .trim()
        .parse::<u64>()
        .map_err(|_| Error::ValidationError(format!("{label}必须是正整数字符串")))?;
    if parsed == 0 {
        return Err(Error::ValidationError(format!("{label}必须大于0")));
    }
    Ok(parsed)
}

fn ensure_evidence_list(values: &[String]) -> Result<()> {
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(Error::ValidationError("所需证据条目不能为空".to_string()));
    }
    let unique = values
        .iter()
        .map(|value| value.trim())
        .collect::<std::collections::HashSet<_>>();
    if unique.len() != values.len() {
        return Err(Error::ValidationError("所需证据条目不能重复".to_string()));
    }
    Ok(())
}

/// 序列化 W17 强命令并构造领域幂等身份。
///
/// # 参数
/// * `actor_id` - 当前操作人
/// * `action` - 稳定命令动作
/// * `resource_id` - 命令资源 ID
/// * `idempotency_key` - 客户端幂等键
/// * `command` - 完整命令载荷
///
/// # 返回
/// 返回审计 ID、载荷指纹与幂等键摘要。
///
/// # 错误
/// 命令无法序列化时返回内部错误。
fn mall_command_identity<T: Serialize>(
    actor_id: &str,
    action: &str,
    resource_id: &str,
    idempotency_key: &str,
    command: &T,
) -> Result<MallSyncCommandIdentity> {
    let payload =
        serde_json::to_vec(command).map_err(|_| Error::Internal("无法形成 W17 命令指纹".to_string()))?;
    Ok(MallSyncCommandIdentity::new(
        W17_RECEIPT_PREFIX,
        actor_id,
        action,
        resource_id,
        idempotency_key,
        &payload,
    ))
}

fn ensure_reapply_receipt(
    operation: &MallSnapshotReapplyOperation,
    mapping_task_id: &str,
    operation_id: &str,
    idempotency_key_hash: &str,
    fingerprint: &str,
) -> Result<()> {
    if operation.matches_replay(mapping_task_id, operation_id, idempotency_key_hash, fingerprint) {
        return Ok(());
    }
    Err(Error::ConflictError(
        "重新归集幂等键已用于不同操作或命令内容".to_string(),
    ))
}

fn command_audit_message(fingerprint: &str) -> String {
    format!("{COMMAND_FINGERPRINT_PREFIX}{fingerprint}")
}

fn audit_command_fingerprint(message: &str) -> Option<&str> {
    message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split(';').next())
        .filter(|value| value.len() == 64)
}

fn ensure_command_receipt(
    audit: &entities::AuditLog,
    action: &str,
    resource_id: &str,
    fingerprint: &str,
) -> Result<()> {
    if !audit.success
        || audit.action != action
        || audit.resource_type != W17_OBJECT_TYPE
        || audit.resource_id.as_deref() != Some(resource_id)
    {
        return Err(Error::Internal("W17 幂等回执与命令资源不一致".to_string()));
    }
    if audit.message.as_deref().and_then(audit_command_fingerprint) != Some(fingerprint) {
        return Err(Error::ConflictError("幂等键已用于不同的命令内容".to_string()));
    }
    Ok(())
}

fn receipt_task_version(message: Option<&str>) -> Result<u64> {
    let value = message
        .and_then(|message| message.split(";task_version=").nth(1))
        .ok_or_else(|| Error::Internal("来源修复回执缺少任务版本".to_string()))?;
    value
        .parse::<u64>()
        .map_err(|_| Error::Internal("来源修复回执任务版本非法".to_string()))
}

/// 为已确定责任角色的映射差异构造唯一正式任务。
fn mapping_work_item(task: &MasterMappingTask, owner_role: String, owner_user_id: &str) -> Result<WorkItem> {
    let sla_seconds = task.mapping_type.sla_seconds();
    Ok(WorkItem::new(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::BusinessException,
            business_object_type: "MASTER_MAPPING_TASK".to_string(),
            business_object_id: task.base.id.clone(),
            subject_version: task.subject_version(),
            owner_role,
            owner_organization_id: "company".to_string(),
            owner_user_id: owner_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(Instant::from_unix_secs(
                Instant::now().unix_secs().saturating_add(sla_seconds),
            )),
            reason_code: Some(format!(
                "MALL_MAPPING_{}",
                task.mapping_type.as_str().to_uppercase()
            )),
            impact_summary: Some(format!("{}主数据映射差异待确认", task.mapping_type.label())),
        },
    )?)
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

#[cfg(test)]
mod tests {
    use entities::mall_sync::{MallSyncTriggerSource, MappingTaskType};
    use entities::source_registry::MallSyncStage;

    use super::{
        ensure_mapping_resolution_shape, ensure_trigger_command, mapping_external_identity,
        trigger_actor_fields, ConfirmMappingCommand, Error, TriggerMallSyncCommand,
    };

    fn confirm_command(object_type: &str) -> ConfirmMappingCommand {
        serde_json::from_value(serde_json::json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "1",
            "expected_subject_version": "1",
            "decision": {
                "mapping_task_id": "mapping-task-1",
                "source_snapshot_id": "snapshot-1",
                "expected_mapping_task_version": 1,
                "mapping_operation_id": "operation-1",
                "execution_stage": "FIRST_PHASE_MALL_OWNED",
                "resolution": {
                    "type": "CONFIRM_TARGET",
                    "object_type": object_type,
                    "object_id": "target-1",
                    "relation_role": "PRIMARY"
                },
                "evidence_note": "已核对规范对象"
            },
            "idempotency_key": "request-1"
        }))
        .unwrap()
    }

    #[test]
    fn resolution_shape_is_fixed_by_mapping_type_registry() {
        assert!(
            ensure_mapping_resolution_shape(MappingTaskType::Customer, &confirm_command("CUSTOMER"),).is_ok()
        );
        assert!(
            ensure_mapping_resolution_shape(MappingTaskType::Contract, &confirm_command("CUSTOMER"),)
                .is_err()
        );
        assert!(
            ensure_mapping_resolution_shape(MappingTaskType::UniqueLineItem, &confirm_command("SKU"),)
                .is_err()
        );
    }

    #[test]
    fn snapshot_adapter_preserves_domain_identity_rules() {
        assert_eq!(
            mapping_external_identity(
                r#"{"customer_external_id":" C-1 ","customer_id":"C-1"}"#,
                MappingTaskType::Customer,
            )
            .unwrap(),
            "C-1"
        );
        assert_eq!(
            mapping_external_identity(r#"{"contract_id":42}"#, MappingTaskType::Contract).unwrap(),
            "42"
        );
        assert!(mapping_external_identity(
            r#"{"contract_external_id":"CT-1","contract_id":"CT-2"}"#,
            MappingTaskType::Contract,
        )
        .is_err());
        assert!(mapping_external_identity(r#"{"contract_id":1.5}"#, MappingTaskType::Contract).is_err());
    }

    #[test]
    fn trigger_gate_rejects_blank_identity_and_archived_stage() {
        let blank_identity = TriggerMallSyncCommand::Incremental {
            source_system_id: entities::ids::SourceSystemId::new("mall-1"),
            execution_stage: MallSyncStage::FirstPhaseMallOwned,
            trigger_source: MallSyncTriggerSource::Manual,
            reason: Some("人工核对".to_string()),
            base_cursor_version: None,
            idempotency_key: " ".to_string(),
        };
        assert!(matches!(
            ensure_trigger_command(&blank_identity),
            Err(Error::ValidationError(_))
        ));

        let archived = TriggerMallSyncCommand::Incremental {
            source_system_id: entities::ids::SourceSystemId::new("mall-1"),
            execution_stage: MallSyncStage::Archived,
            trigger_source: MallSyncTriggerSource::Manual,
            reason: Some("人工核对".to_string()),
            base_cursor_version: None,
            idempotency_key: "request-1".to_string(),
        };
        assert!(matches!(
            ensure_trigger_command(&archived),
            Err(Error::BusinessLogicError(_))
        ));
        assert!(trigger_actor_fields(
            MallSyncTriggerSource::Scheduled,
            Some("不应携带理由"),
            "scheduler",
        )
        .is_err());
    }
}
