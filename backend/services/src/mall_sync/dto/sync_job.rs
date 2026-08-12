use entities::common::time::Instant;
use entities::ids::SourceSystemId;
use entities::mall_sync::{
    MallSalesSyncCursor, MallSalesSyncJob, MallSalesSyncJobStatus, MallSalesSyncJobType,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

use super::common::{normalize_sort, PageParams};

/// 同步作业列表允许的排序字段白名单。
pub(crate) const MALL_SALES_SYNC_JOB_SORT_FIELDS: &[&str] = &["created_at", "started_at"];

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
    use super::MallSalesSyncJobListParams;
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
}
