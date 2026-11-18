//! 批量取消后台任务的权限、状态和逐任务事务编排。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use persistence_core::Transactional;
use tracing::warn;
use validator::Validate;

use super::{BulkJobService, CancelAllBackgroundJobsRequest, CancelAllBackgroundJobsResponse};
use crate::repository::prelude::*;
use crate::{BackgroundJob, BulkJobExt, CancelBackgroundJobFailure, Error, Result};

impl BulkJobService {
    /// 管理员批量取消未完成任务时，可按任务类型收窄候选。先在读取快照验权取候选，再逐个在各自事务中复验权限、读最新版本并取消。
    /// 终态或领域规则禁止取消的任务计为跳过，单项失败按任务 ID 和错误分类返回，不阻塞其他项。非管理员或候选读取失败时拒绝整次调用。
    pub async fn cancel_all_background_jobs(
        &self,
        req: CancelAllBackgroundJobsRequest,
        actor: &AuditActor,
    ) -> Result<CancelAllBackgroundJobsResponse> {
        let candidates = self.cancel_candidates(req, actor).await?;

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

    // 在同一快照检查批量资格并取得当前候选，不把非事务预检当成写入资格。
    async fn cancel_candidates(
        &self,
        req: CancelAllBackgroundJobsRequest,
        actor: &AuditActor,
    ) -> Result<Vec<BackgroundJob>> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization.require_cancel_all(&actor, executor).await?;
                    req.validate()?;
                    Ok(db.background_jobs().cancellation_candidates(req.job_type, executor).await?)
                })
            })
            .await
    }

    // 逐任务在写事务复验管理员资格并读取最新状态；可取消时同事务保存任务和审计，终态或领域拒绝则跳过。
    // 审计、版本竞争或仓储失败由批量入口累计为该任务的失败。
    async fn cancel_candidate(&self, job: BackgroundJob, actor: &AuditActor) -> Result<bool> {
        let id = job.base.id;
        let db = self.db.clone();
        let audit_port = self.audit.clone();
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization.require_cancel_all(&actor, executor).await?;

                    let mut job = db
                        .background_jobs()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("后台任务不存在".into()))?;
                    if job.is_terminal() || job.cancel(Instant::now()).is_err() {
                        return Ok(false);
                    }

                    let audit =
                        audit_port.resource_log(actor, "background_job.cancel_all", "background_job", id)?;
                    db.background_jobs().update(&mut job, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok(true)
                })
            })
            .await
    }
}
