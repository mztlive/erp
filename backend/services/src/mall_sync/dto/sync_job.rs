use entities::common::time::Instant;
use entities::ids::SourceSystemId;
use entities::mall_sync::{
    MallSalesSyncCursor, MallSalesSyncJob, MallSalesSyncJobStatus, MallSalesSyncJobType,
    MallSyncTriggerSource,
};
use entities::source_registry::MallSyncStage;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

use super::common::{normalize_sort, PageParams};

/// 同步作业列表允许的排序字段白名单。
pub(crate) const MALL_SALES_SYNC_JOB_SORT_FIELDS: &[&str] = &["created_at", "started_at"];

/// 每日核对的固定来源边界。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationBoundary {
    /// 来源清单时点。
    pub as_of: Instant,
    /// 来源清单摘要。
    pub source_digest: Option<String>,
}

/// W17 同步触发强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TriggerMallSyncCommand {
    /// 由系统水位与当前安全时间计算范围的增量任务。
    Incremental {
        source_system_id: SourceSystemId,
        execution_stage: MallSyncStage,
        trigger_source: MallSyncTriggerSource,
        reason: Option<String>,
        base_cursor_version: Option<u64>,
        idempotency_key: String,
    },
    /// 沿原来源销售单身份执行的按单补拉。
    SingleOrder {
        source_system_id: SourceSystemId,
        execution_stage: MallSyncStage,
        trigger_source: MallSyncTriggerSource,
        external_order_no: String,
        reason: String,
        idempotency_key: String,
    },
    /// 沿原失败作业创建的新重试尝试。
    RetryFailedJob {
        source_system_id: SourceSystemId,
        execution_stage: MallSyncStage,
        failed_job_id: String,
        reason: String,
        base_cursor_version: Option<u64>,
        idempotency_key: String,
    },
    /// 按固定清单边界执行核对。
    Reconciliation {
        source_system_id: SourceSystemId,
        execution_stage: MallSyncStage,
        reason: String,
        reconciliation_boundary: ReconciliationBoundary,
        idempotency_key: String,
    },
}

/// 失败同步作业重试请求。
///
/// 原作业的来源商城、执行阶段、作业类型与查询范围均由服务端按 `failed_job_id`
/// 重读，调用方不得重复提交这些可推导字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryMallSalesSyncJobRequest {
    /// 人工重试理由。
    pub reason: String,
    /// 可选的同步水位乐观锁版本。
    pub base_cursor_version: Option<u64>,
    /// 本次重试命令幂等键。
    pub idempotency_key: String,
}

impl TriggerMallSyncCommand {
    /// 返回命令指定的来源商城。
    pub fn source_system_id(&self) -> &SourceSystemId {
        match self {
            Self::Incremental { source_system_id, .. }
            | Self::SingleOrder { source_system_id, .. }
            | Self::RetryFailedJob { source_system_id, .. }
            | Self::Reconciliation { source_system_id, .. } => source_system_id,
        }
    }

    /// 返回调用方冻结的执行阶段。
    pub fn execution_stage(&self) -> MallSyncStage {
        match self {
            Self::Incremental { execution_stage, .. }
            | Self::SingleOrder { execution_stage, .. }
            | Self::RetryFailedJob { execution_stage, .. }
            | Self::Reconciliation { execution_stage, .. } => *execution_stage,
        }
    }

    /// 返回命令幂等键。
    pub fn idempotency_key(&self) -> &str {
        match self {
            Self::Incremental { idempotency_key, .. }
            | Self::SingleOrder { idempotency_key, .. }
            | Self::RetryFailedJob { idempotency_key, .. }
            | Self::Reconciliation { idempotency_key, .. } => idempotency_key,
        }
    }
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

impl SyncJobOutcome {
    /// 将请求终态结果映射为作业持久化状态。
    ///
    /// # 参数
    /// 无额外参数；映射仅依赖终态结果本身。
    ///
    /// # 返回
    /// 返回对应的 `MallSalesSyncJobStatus` 终态。
    ///
    /// # 错误
    /// 本映射为全函数，不返回错误。
    ///
    /// # 约束
    /// 纯值映射，不访问数据库；作业状态机推进仍由实体 `finish` 校验。
    pub fn status(self) -> MallSalesSyncJobStatus {
        match self {
            Self::Success => MallSalesSyncJobStatus::Success,
            Self::PartialFailure => MallSalesSyncJobStatus::PartialFailure,
            Self::Failed => MallSalesSyncJobStatus::Failed,
        }
    }
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
    /// 按单补拉的原来源销售单号。
    pub external_order_no: Option<String>,
    /// 触发来源。
    pub trigger_source: MallSyncTriggerSource,
    /// 人工触发理由。
    pub trigger_reason: Option<String>,
    /// 人工触发人。
    pub triggered_by: Option<String>,
    /// 失败重试沿用的原作业。
    pub source_job_id: Option<String>,
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
            external_order_no: job.external_order_no,
            trigger_source: job.trigger_source,
            trigger_reason: job.trigger_reason,
            triggered_by: job.triggered_by,
            source_job_id: job.source_job_id.map(|id| id.to_string()),
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

#[cfg(test)]
mod tests {
    use super::{MallSalesSyncJobListParams, MallSalesSyncJobStatus, SyncJobOutcome, TriggerMallSyncCommand};
    use crate::mall_sync::dto::common::SortDir;
    use validator::Validate;

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
    fn single_order_command_carries_source_identity_and_stage() {
        let command: TriggerMallSyncCommand = serde_json::from_value(serde_json::json!({
            "mode": "SINGLE_ORDER",
            "source_system_id": "mall-1",
            "execution_stage": "FIRST_PHASE_MALL_OWNED",
            "trigger_source": "MANUAL",
            "external_order_no": "MALL-001",
            "reason": "核对差异后补拉",
            "idempotency_key": "request-1"
        }))
        .unwrap();
        assert_eq!(command.source_system_id().as_ref(), "mall-1");
        assert_eq!(command.idempotency_key(), "request-1");
    }

    #[test]
    fn outcome_status_covers_all_branches() {
        assert_eq!(SyncJobOutcome::Success.status(), MallSalesSyncJobStatus::Success);
        assert_eq!(
            SyncJobOutcome::PartialFailure.status(),
            MallSalesSyncJobStatus::PartialFailure
        );
        assert_eq!(SyncJobOutcome::Failed.status(), MallSalesSyncJobStatus::Failed);
    }
}
