//! `background_jobs` 列表筛选、投影行与仓储查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::BackgroundJobId;
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::QueryOutput;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use super::super::page::search_projected_page;
use super::sort_doc;
use crate::dto::bulk_job::BackgroundJobListQuery;
use crate::entity::bulk_job::{BackgroundJob, JobStatus, JobType};
use crate::repository::BulkJobExt;

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
    /// 仅查询未结束的待执行、执行中和部分成功任务。
    pub active_only: bool,
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

impl BackgroundJobFilter {
    // 将本人任务入口固定到认证发起人，保留其余业务筛选。
    // 发起人固定为认证账号的列表筛选。
    // query 为归一化查询，requester 为入口已认证的账号。
    pub(crate) fn for_requester(query: BackgroundJobListQuery, requester: &str) -> Self {
        Self { requested_by: Some(requester.to_owned()), ..Self::from(query) }
    }
}

impl From<BackgroundJobListQuery> for BackgroundJobFilter {
    fn from(query: BackgroundJobListQuery) -> Self {
        let (page, page_size, sort_by, sort_ascending) = query.paging.into_filter_parts();
        Self {
            job_no: query.job_no,
            job_type: query.job_type,
            domain_job_type: query.domain_job_type,
            status: query.status,
            active_only: query.active_only,
            requested_by: query.requested_by,
            page,
            page_size,
            sort_by,
            sort_ascending,
        }
    }
}

/// 在完整授权条件内完成任务分页、计数及前后复验。
/// 同一授权条件内的任务页与总数。
/// db、filter 为任务仓储与业务筛选；authorization、access 为本阶段凭证；executor 为原事务。
/// 授权模型不是完整后台任务、凭证失效、数量越界或数据库失败时拒绝。
pub async fn authorized_background_page(
    db: &Database,
    filter: &BackgroundJobFilter,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> crate::Result<PageResult<BackgroundJobRow>> {
    let model = access.model();
    if model.resource != "background_job"
        || model.key.as_str() != "background_jobs"
        || model.collection != Database::BACKGROUND_JOBS
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(crate::Error::ValidationError("后台任务分页要求完整后台任务模型".into()));
    }
    access.ensure_current(executor)?;

    let collection = db.background_jobs().collection().clone_with_type::<BackgroundJobRow>();
    let options = FindOptions::builder()
        .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
        .skip(filter.skip())
        .limit(filter.limit())
        .projection(background_job_projection())
        .build();

    let items = persistence_core::aggregate::find(
        &collection,
        filter.to_doc(),
        options,
        access.stages().to_vec(),
        executor,
    )
    .await?;
    let total =
        persistence_core::aggregate::count(&collection, filter.to_doc(), access.stages().to_vec(), executor)
            .await?;
    let total = i64::try_from(total).map_err(|_| crate::Error::ValidationError("后台任务总数超限".into()))?;
    Ok(PageResult { items, total })
}

impl Default for BackgroundJobFilter {
    // 初始化后台任务筛选：第一页、每页 20 条，业务筛选默认留空。
    fn default() -> Self {
        Self {
            job_no: None,
            job_type: None,
            domain_job_type: None,
            status: None,
            active_only: false,
            requested_by: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for BackgroundJobFilter {
    // 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    // `job_no` 为 `Some` 时按转义后的字面量、忽略大小写匹配。`job_type` 与 `status` 为 `Some` 时写入 `as_str()`；`domain_job_type` 与 `requested_by` 为 `Some` 时精确匹配。分页与排序字段不进入条件。
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
        if self.active_only {
            filter.insert("finished_at", Bson::Null);
            filter.insert(
                "$and",
                vec![doc! { "status": { "$in": [
                    JobStatus::Pending.as_str(), JobStatus::Running.as_str(),
                    JobStatus::PartiallySucceeded.as_str(),
                ] } }],
            );
        }
        if let Some(requested_by) = &self.requested_by {
            filter.insert("requested_by", requested_by);
        }
        filter
    }
}

impl Pagination for BackgroundJobFilter {
    // 返回后台任务筛选保存的页码与单页条数，不把小于 1 的页码归一成第一页。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 后台任务集合仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait BackgroundJobRepositoryExt {
    /// 批量回读指定任务的当前未删除事实。
    /// 返回匹配任务，顺序由调用方按原分页结果恢复。
    /// `ids` 为当前页任务 ID；空集合不访问数据库；`executor` 为调用方的数据访问执行器。
    /// 数据库读取或游标读取失败时返回错误。
    async fn find_by_ids(
        &self,
        ids: &[BackgroundJobId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>>;

    /// 分页检索后台任务列表（投影查询，任务中心）。
    /// 只返回 [`BackgroundJobRow`] 所需的进度字段，不加载整文档；`job_no` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），
    /// 状态/发起人精确匹配覆盖 `idx_background_jobs_status_created`。
    /// 返回当前页投影行与满足筛选条件的总数。
    /// 沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询、游标读取或计数错误。
    async fn search_background_jobs(
        &self,
        filter: &BackgroundJobFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BackgroundJobRow>>;

    /// 按任务编号查找后台任务。
    /// 查询覆盖 `uk_background_jobs_no` 唯一索引；任务中心按编号精确路由。
    /// 返回匹配的未删除任务；无匹配时返回 `None`。
    /// `job_no` 为任务编号；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询错误。
    async fn find_by_job_no(
        &self,
        job_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>>;

    /// 按请求幂等身份查找后台任务。
    /// 查询覆盖 `uk_background_jobs_request_id` 唯一索引；幂等重试按 `request_id` 定位既有任务（§6.1：涉及资金的变更必须具备幂等键）。
    /// 返回匹配的未删除任务；无匹配时返回 `None`。
    /// `request_id` 为请求幂等身份；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询错误。
    async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>>;
}

impl BackgroundJobRepositoryExt for persistence_core::Repository<'_, BackgroundJob> {
    // 按业务 `id` 做 `$in` 批量读取，未删除过滤沿用通用仓储 `find_many`。
    // 空 `ids` 直接返回空向量，不访问数据库。非空时用各 `BackgroundJobId` 的 `as_ref()` 组成 `$in`，本实现不附加排序。
    // 返回匹配且未删除的后台任务；顺序不保证与输入一致。空输入返回空向量。
    // `ids` 为后台任务 ID；空切片不访问数据库；`executor` 传给 `find_many` 的数据访问执行器。
    // 空输入不失败。否则返回 `find_many` 的错误，即 MongoDB 查询或游标读取失败。
    async fn find_by_ids(
        &self,
        ids: &[BackgroundJobId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = ids.iter().map(|id| id.as_ref()).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

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
        search_projected_page(&self.collection(), filter, options, executor).await
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

// 后台任务列表投影字段。
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
    use mongodb::bson::Bson;
    use persistence_core::QueryFilter;

    use super::BackgroundJobFilter;
    use crate::entity::bulk_job::{JobStatus, JobType};

    #[test]
    fn active_filter_excludes_finished_jobs_and_preserves_owner_and_status() {
        let filter = BackgroundJobFilter {
            active_only: true,
            requested_by: Some("actor-1".to_string()),
            status: Some(JobStatus::PartiallySucceeded),
            ..Default::default()
        };
        let document = filter.to_doc();
        assert_eq!(document.get("finished_at"), Some(&Bson::Null));
        assert_eq!(document.get_str("requested_by").unwrap(), "actor-1");
        assert_eq!(document.get_str("status").unwrap(), "partially_succeeded");
        let active = document.get_array("$and").unwrap()[0].as_document().unwrap();
        let statuses = active.get_document("status").unwrap().get_array("$in").unwrap();
        assert_eq!(
            statuses.iter().map(Bson::as_str).collect::<Vec<_>>(),
            vec![Some("pending"), Some("running"), Some("partially_succeeded")]
        );

        let all = BackgroundJobFilter::default().to_doc();
        assert!(!all.contains_key("finished_at"));
        assert!(!all.contains_key("$and"));
    }

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
    #[test]
    fn own_entry_ignores_requested_owner_and_keeps_domain_filter() {
        let params = crate::BackgroundJobListParams {
            requested_by: Some("another-user".into()),
            domain_job_type: Some("DATA_TRANSFER".into()),
            ..Default::default()
        };
        let filter = BackgroundJobFilter::for_requester(params.normalized().unwrap(), "authenticated");
        let document = filter.to_doc();
        assert_eq!(document.get_str("requested_by").unwrap(), "authenticated");
        assert_eq!(document.get_str("domain_job_type").unwrap(), "DATA_TRANSFER");
    }
}
