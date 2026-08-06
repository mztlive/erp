//! 域 D23 `mall_sync` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。
//!
//! 快照落盘契约（数据模型 §6.13）：来源单号、商城更新时间、状态码、规范化快照
//! 由同步调用方提供；来源商城与观察时间由服务端从作业上下文注入，禁止客户端
//! 伪造来源归属。

use entities::common::time::Instant;
use entities::ids::{
    MallSalesOrderSnapshotId, MallSalesReconciliationJobId, MallSalesSyncJobId, SalesOrderId,
    SalesOrderRevisionId, SourceSystemId,
};
use entities::mall_sync::{
    MallSalesOrderSnapshot, MallSalesReconciliationItem, MallSalesReconciliationJob, MallSalesSyncCursor,
    MallSalesSyncJob, MallSalesSyncJobStatus, MallSalesSyncJobType, MappingTaskStatus, MappingTaskType,
    MasterMappingTask, ReconciliationDifferenceType, ReconciliationItemStatus, ReconciliationJobStatus,
    SnapshotMappingStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 同步作业列表允许的排序字段白名单。
pub(crate) const MALL_SALES_SYNC_JOB_SORT_FIELDS: &[&str] = &["created_at", "started_at"];
/// 快照列表允许的排序字段白名单。
pub(crate) const MALL_SALES_ORDER_SNAPSHOT_SORT_FIELDS: &[&str] =
    &["created_at", "source_updated_at", "observed_at"];
/// 核对作业列表允许的排序字段白名单。
pub(crate) const MALL_SALES_RECONCILIATION_JOB_SORT_FIELDS: &[&str] = &["created_at", "source_list_as_of"];
/// 核对差异明细列表允许的排序字段白名单。
pub(crate) const MALL_SALES_RECONCILIATION_ITEM_SORT_FIELDS: &[&str] = &["created_at", "source_updated_at"];
/// 映射任务列表允许的排序字段白名单。
pub(crate) const MASTER_MAPPING_TASK_SORT_FIELDS: &[&str] = &["created_at", "resolved_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询参数（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 同步作业创建请求（数据模型 §6.13；区间必须成对，实体层校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateMallSalesSyncJobRequest {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 作业类型。
    pub job_type: MallSalesSyncJobType,
    /// 本次查询时间边界起（单号补拉等无区间任务为空）。
    pub range_start: Option<Instant>,
    /// 本次查询时间边界止（与 `range_start` 必须成对出现）。
    pub range_end: Option<Instant>,
}

/// 同步作业完成请求（终态结果；`Success` 要求错误计数为零，实体层校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompleteMallSalesSyncJobRequest {
    /// 终态结果。
    pub outcome: SyncJobOutcome,
}

/// 同步作业终态结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncJobOutcome {
    /// 成功。
    Success,
    /// 部分失败。
    PartialFailure,
    /// 失败。
    Failed,
}

/// 同步作业响应视图（字段与数据模型 §6.13 一致）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesSyncJobView {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: String,
    /// 作业类型。
    pub job_type: MallSalesSyncJobType,
    /// 本次查询时间边界起。
    pub range_start: Option<Instant>,
    /// 本次查询时间边界止。
    pub range_end: Option<Instant>,
    /// 任务开始时间。
    pub started_at: Instant,
    /// 任务结束时间。
    pub finished_at: Option<Instant>,
    /// 作业状态。
    pub status: MallSalesSyncJobStatus,
    /// 处理页数。
    pub page_count: u64,
    /// 处理条数。
    pub item_count: u64,
    /// 错误条数。
    pub error_count: u64,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesSyncJob> for MallSalesSyncJobView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `job` - 同步作业实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(job: MallSalesSyncJob) -> Self {
        Self {
            id: job.base.id,
            source_system_id: job.source_system_id.to_string(),
            job_type: job.job_type,
            range_start: job.range_start,
            range_end: job.range_end,
            started_at: job.started_at,
            finished_at: job.finished_at,
            status: job.status,
            page_count: job.page_count,
            item_count: job.item_count,
            error_count: job.error_count,
            version: job.base.version,
            created_at: job.base.created_at,
        }
    }
}

/// 同步作业列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallSalesSyncJobListParams {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 作业类型筛选。
    pub job_type: Option<MallSalesSyncJobType>,
    /// 作业状态筛选。
    pub status: Option<MallSalesSyncJobStatus>,
    /// 任务开始时间起（含）。
    pub started_at_from: Option<Instant>,
    /// 任务开始时间止（含）。
    pub started_at_to: Option<Instant>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`started_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的同步作业列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallSalesSyncJobListQuery {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 作业类型筛选。
    pub job_type: Option<MallSalesSyncJobType>,
    /// 作业状态筛选。
    pub status: Option<MallSalesSyncJobStatus>,
    /// 任务开始时间起（含）。
    pub started_at_from: Option<Instant>,
    /// 任务开始时间止（含）。
    pub started_at_to: Option<Instant>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallSalesSyncJobListParams {
    /// 归一化同步作业列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallSalesSyncJobListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, MALL_SALES_SYNC_JOB_SORT_FIELDS)?;
        Ok(MallSalesSyncJobListQuery {
            source_system_id: self.source_system_id.clone(),
            job_type: self.job_type,
            status: self.status,
            started_at_from: self.started_at_from,
            started_at_to: self.started_at_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 单条快照入参（来源单身份 + 商城侧值；来源商城与观察时间由服务端注入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SnapshotItemRequest {
    /// 一期来源单号原值。
    #[validate(custom(function = "non_blank", message = "来源单号不能为空"))]
    pub external_order_no: String,
    /// 商城更新时间（秒级时间戳）。
    pub source_updated_at: Instant,
    /// 商业事实投影指纹（可选列，仅用于变更判断）。
    pub content_hash: Option<String>,
    /// 商城当前状态码。
    #[validate(custom(function = "non_blank", message = "商城状态码不能为空"))]
    pub source_status_code: String,
    /// 规范化外部快照归档。
    #[validate(custom(function = "non_blank", message = "规范化快照不能为空"))]
    pub normalized_snapshot: String,
    /// 可选的加密原始报文引用。
    pub raw_payload_reference: Option<String>,
}

/// 快照落盘请求（一次一页；`(来源商城, 比较键, 商城更新时间)` 唯一，重复推送按幂等跳过）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IngestMallSalesOrderSnapshotsRequest {
    /// 来源同步作业。
    pub sync_job_id: MallSalesSyncJobId,
    /// 本页快照。
    #[validate(length(min = 1, max = 500, message = "快照数量必须在1-500之间"))]
    pub items: Vec<SnapshotItemRequest>,
}

/// 快照落盘结果（重复/迟到快照计入 `skipped`，不产生重复事实）。
#[derive(Debug, Clone, Serialize)]
pub struct IngestMallSalesOrderSnapshotsResult {
    /// 本页实际落盘条数。
    pub accepted: u64,
    /// 本页跳过条数（事实键重复或早于最新快照）。
    pub skipped: u64,
    /// 已落盘快照 ID。
    pub snapshot_ids: Vec<String>,
}

/// 商城销售单快照响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesOrderSnapshotView {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: String,
    /// 一期来源单号原值。
    pub external_order_no: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 商业事实投影指纹。
    pub content_hash: Option<String>,
    /// 商城当前状态码。
    pub source_status_code: String,
    /// ERP 实际观察时间。
    pub observed_at: Instant,
    /// 映射状态。
    pub mapping_status: SnapshotMappingStatus,
    /// 成功形成的销售版本。
    pub applied_sales_order_revision_id: Option<String>,
    /// 来源任务。
    pub sync_job_id: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesOrderSnapshot> for MallSalesOrderSnapshotView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `snapshot` - 快照实体
    ///
    /// # 返回
    /// 返回响应视图（不暴露比较键与原始报文引用）。
    fn from(snapshot: MallSalesOrderSnapshot) -> Self {
        Self {
            id: snapshot.base.id,
            source_system_id: snapshot.source_system_id.to_string(),
            external_order_no: snapshot.external_order_no,
            source_updated_at: snapshot.source_updated_at,
            content_hash: snapshot.content_hash,
            source_status_code: snapshot.source_status_code,
            observed_at: snapshot.observed_at,
            mapping_status: snapshot.mapping_status,
            applied_sales_order_revision_id: snapshot
                .applied_sales_order_revision_id
                .map(|id| id.to_string()),
            sync_job_id: snapshot.sync_job_id.to_string(),
            version: snapshot.base.version,
            created_at: snapshot.base.created_at,
        }
    }
}

/// 快照列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallSalesOrderSnapshotListParams {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 映射状态筛选。
    pub mapping_status: Option<SnapshotMappingStatus>,
    /// 观察时间起（含）。
    pub observed_at_from: Option<Instant>,
    /// 观察时间止（含）。
    pub observed_at_to: Option<Instant>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`source_updated_at`/`observed_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的快照列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallSalesOrderSnapshotListQuery {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 映射状态筛选。
    pub mapping_status: Option<SnapshotMappingStatus>,
    /// 观察时间起（含）。
    pub observed_at_from: Option<Instant>,
    /// 观察时间止（含）。
    pub observed_at_to: Option<Instant>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallSalesOrderSnapshotListParams {
    /// 归一化快照列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallSalesOrderSnapshotListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            MALL_SALES_ORDER_SNAPSHOT_SORT_FIELDS,
        )?;
        Ok(MallSalesOrderSnapshotListQuery {
            source_system_id: self.source_system_id.clone(),
            mapping_status: self.mapping_status,
            observed_at_from: self.observed_at_from,
            observed_at_to: self.observed_at_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 同步水位游标响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesSyncCursorView {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: String,
    /// 已安全处理的商城更新时间高水位。
    pub high_water_updated_at: Instant,
    /// 最近成功任务。
    pub last_success_job_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesSyncCursor> for MallSalesSyncCursorView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `cursor` - 水位游标实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(cursor: MallSalesSyncCursor) -> Self {
        Self {
            id: cursor.base.id,
            source_system_id: cursor.source_system_id.to_string(),
            high_water_updated_at: cursor.high_water_updated_at,
            last_success_job_id: cursor.last_success_job_id.map(|id| id.to_string()),
            version: cursor.base.version,
            created_at: cursor.base.created_at,
        }
    }
}

/// 核对差异明细入参（差异类型与 ERP 侧存在性一致性由实体层校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReconciliationItemRequest {
    /// 来源单号。
    #[validate(custom(function = "non_blank", message = "来源单号不能为空"))]
    pub external_order_no: String,
    /// 商城当前状态码。
    #[validate(custom(function = "non_blank", message = "商城状态码不能为空"))]
    pub source_status_code: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 商城内容指纹。
    pub source_content_hash: Option<String>,
    /// ERP 当前正式销售单 ID（`ERP 缺失` 不得携带）。
    pub sales_order_id: Option<SalesOrderId>,
    /// ERP 当前正式销售版本 ID。
    pub erp_revision_id: Option<SalesOrderRevisionId>,
    /// ERP 内容指纹。
    pub erp_content_hash: Option<String>,
    /// 差异类型。
    pub difference_type: ReconciliationDifferenceType,
}

/// 核对作业创建请求（`job_no` 唯一，重复提交按幂等返回既有作业）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateMallSalesReconciliationJobRequest {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 核对批次号（唯一）。
    #[validate(custom(function = "non_blank", message = "核对批次号不能为空"))]
    pub job_no: String,
    /// 商城全量清单边界。
    pub source_list_as_of: Instant,
    /// 商城清单数量。
    pub source_count: u64,
    /// ERP 数量。
    pub erp_count: u64,
    /// 差异明细（差异数量 = 明细条数）。
    #[validate(length(min = 1, max = 1000, message = "差异明细数量必须在1-1000之间"))]
    pub items: Vec<ReconciliationItemRequest>,
}

/// 核对作业响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesReconciliationJobView {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: String,
    /// 核对批次号。
    pub job_no: String,
    /// 商城全量清单边界。
    pub source_list_as_of: Instant,
    /// 商城清单数量。
    pub source_count: u64,
    /// ERP 数量。
    pub erp_count: u64,
    /// 差异数量。
    pub difference_count: u64,
    /// 作业状态。
    pub status: ReconciliationJobStatus,
    /// 任务开始时间。
    pub started_at: Instant,
    /// 任务结束时间。
    pub finished_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesReconciliationJob> for MallSalesReconciliationJobView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `job` - 核对作业实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(job: MallSalesReconciliationJob) -> Self {
        Self {
            id: job.base.id,
            source_system_id: job.source_system_id.to_string(),
            job_no: job.job_no,
            source_list_as_of: job.source_list_as_of,
            source_count: job.source_count,
            erp_count: job.erp_count,
            difference_count: job.difference_count,
            status: job.status,
            started_at: job.started_at,
            finished_at: job.finished_at,
            version: job.base.version,
            created_at: job.base.created_at,
        }
    }
}

/// 核对作业列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallSalesReconciliationJobListParams {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 作业状态筛选。
    pub status: Option<ReconciliationJobStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`source_list_as_of`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的核对作业列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallSalesReconciliationJobListQuery {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 作业状态筛选。
    pub status: Option<ReconciliationJobStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallSalesReconciliationJobListParams {
    /// 归一化核对作业列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallSalesReconciliationJobListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            MALL_SALES_RECONCILIATION_JOB_SORT_FIELDS,
        )?;
        Ok(MallSalesReconciliationJobListQuery {
            source_system_id: self.source_system_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 核对差异明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesReconciliationItemView {
    /// 实体主键。
    pub id: String,
    /// 所属核对作业。
    pub reconciliation_job_id: String,
    /// 来源单号。
    pub external_order_no: String,
    /// 商城当前状态码。
    pub source_status_code: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 差异类型。
    pub difference_type: ReconciliationDifferenceType,
    /// 明细状态。
    pub status: ReconciliationItemStatus,
    /// 按单号补拉任务。
    pub single_order_sync_job_id: Option<String>,
    /// 人工处理结论。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesReconciliationItem> for MallSalesReconciliationItemView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `item` - 差异明细实体
    ///
    /// # 返回
    /// 返回响应视图（不暴露比较键）。
    fn from(item: MallSalesReconciliationItem) -> Self {
        Self {
            id: item.base.id,
            reconciliation_job_id: item.reconciliation_job_id.to_string(),
            external_order_no: item.external_order_no,
            source_status_code: item.source_status_code,
            source_updated_at: item.source_updated_at,
            difference_type: item.difference_type,
            status: item.status,
            single_order_sync_job_id: item.single_order_sync_job_id.map(|id| id.to_string()),
            resolution: item.resolution,
            resolved_by: item.resolved_by,
            resolved_at: item.resolved_at,
            version: item.base.version,
            created_at: item.base.created_at,
        }
    }
}

/// 核对差异明细列表查询参数（按核对作业查询为主）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallSalesReconciliationItemListParams {
    /// 所属核对作业。
    pub reconciliation_job_id: Option<MallSalesReconciliationJobId>,
    /// 明细状态筛选。
    pub status: Option<ReconciliationItemStatus>,
    /// 差异类型筛选。
    pub difference_type: Option<ReconciliationDifferenceType>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`source_updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的核对差异明细列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallSalesReconciliationItemListQuery {
    /// 所属核对作业。
    pub reconciliation_job_id: Option<MallSalesReconciliationJobId>,
    /// 明细状态筛选。
    pub status: Option<ReconciliationItemStatus>,
    /// 差异类型筛选。
    pub difference_type: Option<ReconciliationDifferenceType>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallSalesReconciliationItemListParams {
    /// 归一化核对差异明细列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallSalesReconciliationItemListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            MALL_SALES_RECONCILIATION_ITEM_SORT_FIELDS,
        )?;
        Ok(MallSalesReconciliationItemListQuery {
            reconciliation_job_id: self.reconciliation_job_id.clone(),
            status: self.status,
            difference_type: self.difference_type,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 差异明细处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveItemKind {
    /// 人工解决（必须携带处理结论）。
    Resolve,
    /// 补拉后确认无误（不要求处理结论）。
    ConfirmNoDifference,
}

/// 差异明细处理请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveMallSalesReconciliationItemRequest {
    /// 处理方式。
    pub kind: ResolveItemKind,
    /// 人工处理结论（`resolve` 必填）。
    pub resolution: Option<String>,
}

/// 映射任务创建请求（同一快照、映射类型只允许一个进行中任务）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateMasterMappingTaskRequest {
    /// 待处理快照。
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 业务责任角色。
    #[validate(custom(function = "non_blank", message = "责任角色不能为空"))]
    pub owner_role: String,
    /// 业务责任用户 ID（可按角色领办，可为空）。
    pub owner_user_id: Option<String>,
}

/// 映射任务响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MasterMappingTaskView {
    /// 实体主键。
    pub id: String,
    /// 待处理快照。
    pub source_snapshot_id: String,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 任务状态。
    pub status: MappingTaskStatus,
    /// 业务责任角色。
    pub owner_role: String,
    /// 业务责任用户 ID。
    pub owner_user_id: Option<String>,
    /// 处理结论。
    pub resolution: Option<String>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MasterMappingTask> for MasterMappingTaskView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `task` - 映射任务实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(task: MasterMappingTask) -> Self {
        Self {
            id: task.base.id,
            source_snapshot_id: task.source_snapshot_id.to_string(),
            mapping_type: task.mapping_type,
            status: task.status,
            owner_role: task.owner_role,
            owner_user_id: task.owner_user_id,
            resolution: task.resolution,
            resolved_at: task.resolved_at,
            version: task.base.version,
            created_at: task.base.created_at,
        }
    }
}

/// 映射任务列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MasterMappingTaskListParams {
    /// 待处理快照筛选。
    pub source_snapshot_id: Option<MallSalesOrderSnapshotId>,
    /// 映射类型筛选。
    pub mapping_type: Option<MappingTaskType>,
    /// 任务状态筛选。
    pub status: Option<MappingTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 责任用户 ID 筛选。
    pub owner_user_id: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`resolved_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的映射任务列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MasterMappingTaskListQuery {
    /// 待处理快照筛选。
    pub source_snapshot_id: Option<MallSalesOrderSnapshotId>,
    /// 映射类型筛选。
    pub mapping_type: Option<MappingTaskType>,
    /// 任务状态筛选。
    pub status: Option<MappingTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 责任用户 ID 筛选。
    pub owner_user_id: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MasterMappingTaskListParams {
    /// 归一化映射任务列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MasterMappingTaskListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, MASTER_MAPPING_TASK_SORT_FIELDS)?;
        Ok(MasterMappingTaskListQuery {
            source_snapshot_id: self.source_snapshot_id.clone(),
            mapping_type: self.mapping_type,
            status: self.status,
            owner_role: normalized_text(self.owner_role.as_deref()),
            owner_user_id: normalized_text(self.owner_user_id.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 映射任务处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveTaskKind {
    /// 已解决。
    Resolved,
    /// 无法处理。
    Unresolvable,
}

/// 映射任务处理请求（处理结论必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveMasterMappingTaskRequest {
    /// 处理方式。
    pub kind: ResolveTaskKind,
    /// 处理结论（映射结果说明或无法处理原因）。
    #[validate(custom(function = "non_blank", message = "处理结论不能为空"))]
    pub resolution: String,
}

/// 来源系统校验错误文案（D01 读取失败时使用）。
pub(crate) const SOURCE_SYSTEM_NOT_FOUND_MESSAGE: &str = "来源系统不存在";
/// ERP 销售单缺失错误文案（D13 读取失败时使用）。
pub(crate) const SALES_ORDER_NOT_FOUND_MESSAGE: &str = "ERP 销售单不存在";
/// ERP 销售单客户缺失错误文案（D08 读取失败时使用）。
pub(crate) const SALES_ORDER_CUSTOMER_MISSING_MESSAGE: &str = "ERP 销售单关联的客户账号不存在";

#[cfg(test)]
mod tests {
    use super::{normalize_sort, MallSalesSyncJobListParams, SortDir};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("id".to_string()), &None, &["created_at", "started_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" started_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "started_at"],
        )
        .unwrap();
        assert_eq!(field, "started_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn job_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = MallSalesSyncJobListParams {
            source_system_id: None,
            job_type: None,
            status: None,
            started_at_from: None,
            started_at_to: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("started_at".to_string()),
            sort_dir: Some("desc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "started_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = MallSalesSyncJobListParams {
            source_system_id: None,
            job_type: None,
            status: None,
            started_at_from: None,
            started_at_to: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn ingest_request_rejects_empty_items() {
        let request: super::IngestMallSalesOrderSnapshotsRequest =
            serde_json::from_value(serde_json::json!({ "sync_job_id": "j-1", "items": [] })).unwrap();
        assert!(request.validate().is_err());
    }
}

// gate-rebuild marker

// gate-rebuild marker
