use super::{dto::SupplierConnectionJobView, SupplierApiReadService};
use crate::{Error, Result};
use erp_support::{
    BackgroundJob, BulkJobExt, SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE,
};
use persistence_core::NoTransaction;
impl SupplierApiReadService {
    /// 查询连接下健康检查或目录同步后台任务的当前终态/进度。
    ///
    /// # Errors
    /// 任务不存在或不属于指定连接时返回 `NotFound`。
    pub async fn connection_job(
        &self,
        connection_id: &str,
        job_id: &str,
    ) -> Result<SupplierConnectionJobView> {
        let job = self
            .db
            .background_jobs()
            .find_supplier_connection_job(
                connection_id,
                job_id,
                &[SUPPLIER_HEALTH_CHECK_JOB_TYPE, SUPPLIER_CATALOG_SYNC_JOB_TYPE],
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("连接后台任务不存在".to_string()))?;
        Ok(job_view(job))
    }
}
fn job_view(job: BackgroundJob) -> SupplierConnectionJobView {
    SupplierConnectionJobView {
        job_id: job.base.id,
        job_no: job.job_no,
        action: job.domain_job_type.unwrap_or_default(),
        status: job.status,
        total: job.total_count,
        processed: job.processed_count,
        succeeded: job.success_count,
        failed: job.failed_count,
        error_summary: job.error_summary,
        created_at: job.base.created_at,
        finished_at: job.finished_at.map(|at| at.unix_secs() as u64),
    }
}
