//! 商品导入任务领取与逐项全量读取。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::entity::bulk_job::{BackgroundJob, BackgroundJobId, BackgroundJobItem, JobStatus};
use crate::repository::owned::{BackgroundJobItemRepository, BackgroundJobRepository};
use persistence_core::mongo_ops;
use persistence_core::Executor;
use persistence_core::Result;

impl<'a> BackgroundJobRepository<'a> {
    /// 读取指定领域类型且尚未终态的后台任务，供 worker 领取。
    ///
    /// # 参数
    /// * `domain_job_type` - 领域任务类型
    /// * `limit` - 最多返回条数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 按创建时间升序的待处理任务。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn list_open_by_domain_job_type(
        &self,
        domain_job_type: &str,
        limit: i64,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>> {
        let filter = doc! {
            "domain_job_type": domain_job_type,
            "status": {
                "$in": [
                    JobStatus::Pending.as_str(),
                    JobStatus::Running.as_str(),
                    JobStatus::PartiallySucceeded.as_str(),
                ]
            },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1, "id": 1 })
            .limit(limit)
            .build();
        mongo_ops::find_many(&self.collection(), filter, options, executor).await
    }
}

impl<'a> BackgroundJobItemRepository<'a> {
    /// 读取一个后台任务的全部逐项实体，按序号升序。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回该任务全部逐项记录。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn list_entities_by_job(
        &self,
        job_id: &BackgroundJobId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJobItem>> {
        let filter = doc! {
            "background_job_id": job_id.to_string(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        let options = FindOptions::builder().sort(doc! { "item_no": 1 }).build();
        mongo_ops::find_many(&self.collection(), filter, options, executor).await
    }
}
