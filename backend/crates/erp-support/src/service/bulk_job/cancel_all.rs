//! 批量取消后台任务的权限、状态和逐任务事务编排。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use persistence_core::{NoTransaction, Transactional};
use tracing::warn;
use validator::Validate;

use super::{BulkJobService, CancelAllBackgroundJobsRequest, CancelAllBackgroundJobsResponse};
use crate::repository::prelude::*;
use crate::{BackgroundJob, BulkJobExt, CancelBackgroundJobFailure, Error, Result};

impl BulkJobService {
    /// 停止并取消全部未完成后台任务（仅管理员）。
    ///
    /// 逐个按最新版本取消，已进入终态的不再触碰；单个任务失败不阻塞其余任务，
    /// 调用方可按 `failed_count`/`failures`（任务 id + 错误分类）决定是否重试。
    ///
    /// # 参数
    /// * `req` - 批量取消请求（可选 `job_type` 缩小范围）
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `is_admin` - 是否为管理员（超级管理员或系统管理员）
    ///
    /// # 返回
    /// 返回已取消、跳过与失败的任务数及失败明细。
    ///
    /// # 错误
    /// * `Forbidden` - 非管理员调用
    /// * `RepositoryError` - 批量查询失败
    pub async fn cancel_all_background_jobs(
        &self,
        req: CancelAllBackgroundJobsRequest,
        actor: &AuditActor,
        is_admin: bool,
    ) -> Result<CancelAllBackgroundJobsResponse> {
        if !is_admin {
            return Err(Error::Forbidden("只有管理员可以停止全部后台任务".to_string()));
        }
        req.validate()?;
        let candidates =
            self.db.background_jobs().cancellation_candidates(req.job_type, &mut NoTransaction).await?;
        let mut result = CancelAllBackgroundJobsResponse {
            cancelled_count: 0,
            skipped_count: 0,
            failed_count: 0,
            failures: Vec::new(),
        };
        for job in candidates {
            let job_id = job.base.id.clone();
            match self.cancel_candidate(job, actor).await {
                Ok(true) => result.cancelled_count += 1,
                Ok(false) => result.skipped_count += 1,
                Err(error) => {
                    warn!(job_id = %job_id, error = %error, "批量取消后台任务失败");
                    result.failed_count += 1;
                    result.failures.push(CancelBackgroundJobFailure::from_error(job_id, &error));
                },
            }
        }
        Ok(result)
    }

    /// 单个候选任务取消与审计同事务写入；终态或不允许取消时返回跳过。
    ///
    /// 审计构造、版本竞争或事务失败返回错误，由批量入口累计失败并继续处理其他任务。
    async fn cancel_candidate(&self, mut job: BackgroundJob, actor: &AuditActor) -> Result<bool> {
        if job.is_terminal() || job.cancel(Instant::now()).is_err() {
            return Ok(false);
        }
        let audit = self.audit.resource_log(
            actor.clone(),
            "background_job.cancel_all",
            "background_job",
            job.base.id.clone(),
        )?;
        let db = self.db.clone();
        let audit_port = self.audit.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.background_jobs().update(&mut job, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        Ok(true)
    }
}
