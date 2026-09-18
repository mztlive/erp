//! `background_jobs` 列表筛选、投影行与仓储查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use super::super::page::search_projected_page;
use super::sort_doc;
use crate::entity::bulk_job::{BackgroundJob, JobStatus, JobType};

/// 后台任务列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobRow {
    /// 实体主键。
    pub id: String,
    /// 任务编号。
    pub job_no: String,
    /// 任务类型。
    pub job_type: JobType,
    /// 关联强类型领域任务类型代码。
    pub domain_job_type: Option<String>,
    /// 关联强类型领域任务 ID。
    pub domain_job_id: Option<String>,
    /// 批量或导出使用的不可变选择快照。
    pub selection_snapshot_id: Option<String>,
    /// 任务状态。
    pub status: JobStatus,
    /// 发起人。
    pub requested_by: String,
    /// 请求幂等身份。
    pub request_id: String,
    /// 合规输入包文件资产。
    pub input_file_asset_id: Option<String>,
    /// 结果文件资产。
    pub result_file_asset_id: Option<String>,
    /// 目标总数。
    pub total_count: u64,
    /// 已处理数。
    pub processed_count: u64,
    /// 成功数。
    pub success_count: u64,
    /// 跳过数。
    pub skipped_count: u64,
    /// 失败数。
    pub failed_count: u64,
    /// 开始执行时间（秒级时间戳）。
    pub started_at: Option<u64>,
    /// 结束时间（秒级时间戳）。
    pub finished_at: Option<u64>,
    /// 最近进度时间（秒级时间戳）。
    pub last_progress_at: Option<u64>,
    /// 结果下载到期时间（秒级时间戳）。
    pub result_expires_at: Option<u64>,
    /// 脱敏任务级错误摘要。
    pub error_summary: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 后台任务列表筛选条件。
#[derive(Debug, Clone)]
pub struct BackgroundJobFilter {
    /// 任务编号（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub job_no: Option<String>,
    /// 任务类型；`None` 表示不筛选。
    pub job_type: Option<JobType>,
    /// 领域任务类型；`None` 表示不筛选。
    pub domain_job_type: Option<String>,
    /// 任务状态；`None` 表示不筛选。
    pub status: Option<JobStatus>,
    /// 发起人；`None` 表示不筛选。
    pub requested_by: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for BackgroundJobFilter {
    /// 缺省分页从第一页、每页二十条开始，其余筛选保持空条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回第 1 页、每页 20 条的空筛选条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            job_no: None,
            job_type: None,
            domain_job_type: None,
            status: None,
            requested_by: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for BackgroundJobFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "job_no", self.job_no.as_deref());
        if let Some(job_type) = self.job_type {
            filter.insert("job_type", job_type.as_str());
        }
        if let Some(domain_job_type) = &self.domain_job_type {
            filter.insert("domain_job_type", domain_job_type);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(requested_by) = &self.requested_by {
            filter.insert("requested_by", requested_by);
        }
        filter
    }
}

impl Pagination for BackgroundJobFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 后台任务集合仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait BackgroundJobRepositoryExt {
    /// 分页检索后台任务列表（投影查询，任务中心）。
    ///
    /// 只返回 [`BackgroundJobRow`] 所需的进度字段，不加载整文档；
    /// `job_no` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），
    /// 状态/发起人精确匹配覆盖 `idx_background_jobs_status_created`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_background_jobs(
        &self,
        filter: &BackgroundJobFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BackgroundJobRow>>;

    /// 按任务编号查找后台任务。
    ///
    /// 查询覆盖 `uk_background_jobs_no` 唯一索引；任务中心按编号精确路由。
    ///
    /// # 参数
    /// * `job_no` - 任务编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除任务；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_job_no(
        &self,
        job_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>>;

    /// 按请求幂等身份查找后台任务。
    ///
    /// 查询覆盖 `uk_background_jobs_request_id` 唯一索引；幂等重试按
    /// `request_id` 定位既有任务（§6.1：涉及资金的变更必须具备幂等键）。
    ///
    /// # 参数
    /// * `request_id` - 请求幂等身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除任务；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>>;
}

impl BackgroundJobRepositoryExt for persistence_core::Repository<'_, BackgroundJob> {
    async fn search_background_jobs(
        &self,
        filter: &BackgroundJobFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BackgroundJobRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(background_job_projection())
            .build();
        let collection = self.collection().clone_with_type::<BackgroundJobRow>();
        search_projected_page(&self.collection(), &collection, filter, options, executor).await
    }

    async fn find_by_job_no(
        &self,
        job_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        self.find_one_by_field("job_no", job_no, executor).await
    }

    async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        self.find_one_by_field("request_id", request_id, executor).await
    }
}

/// 后台任务列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn background_job_projection() -> Document {
    doc! {
        "id": 1,
        "job_no": 1,
        "job_type": 1,
        "domain_job_type": 1,
        "domain_job_id": 1,
        "selection_snapshot_id": 1,
        "status": 1,
        "requested_by": 1,
        "request_id": 1,
        "input_file_asset_id": 1,
        "result_file_asset_id": 1,
        "total_count": 1,
        "processed_count": 1,
        "success_count": 1,
        "skipped_count": 1,
        "failed_count": 1,
        "started_at": 1,
        "finished_at": 1,
        "last_progress_at": 1,
        "result_expires_at": 1,
        "error_summary": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::BackgroundJobFilter;
    use crate::entity::bulk_job::{JobStatus, JobType};

    #[test]
    fn background_job_filter_applies_no_regex_and_status() {
        let filter = BackgroundJobFilter {
            job_no: Some("job-001".to_string()),
            job_type: Some(JobType::Import),
            status: Some(JobStatus::Running),
            ..Default::default()
        };

        let document = filter.to_doc();
        let no = document.get_document("job_no").unwrap();
        assert_eq!(no.get_str("$regex").unwrap(), r"job\-001");
        assert_eq!(document.get_str("job_type").unwrap(), "import");
        assert_eq!(document.get_str("status").unwrap(), "running");
    }
}
