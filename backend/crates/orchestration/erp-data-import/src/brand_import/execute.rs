//! 有界认领与恢复，每行在同一事务提交品牌、审计、结果和任务进度。
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use background_runtime::{BackgroundTaskAdapter, TaskResult};
use erp_catalog::dto::BrandImportJobRequest;
use erp_core::common::time::Instant;
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BackgroundJobId, BackgroundJobItem, BulkJobExt, JobStatus};
use persistence_core::{Executor, ReadExecutor, Transactional};

use super::rows::{BrandRowExecution, execute_rows, record};
use super::{BRAND_IMPORT_DOMAIN, BrandImportProcess, authorize, job_actor};
use crate::background::import_budget::import_budget;
use crate::{Error, Result};

#[async_trait]
impl BackgroundTaskAdapter for BrandImportProcess {
    fn name(&self) -> &'static str {
        "brand_import"
    }

    async fn run_due(&self) -> TaskResult<usize> {
        let jobs = self.sources.poll_jobs(&self.db, BRAND_IMPORT_DOMAIN).await?;
        let count = jobs.len();
        for mut job in jobs {
            if let Err(error) = self.execute(&mut job).await
                && !matches!(error, Error::ConflictError(_) | Error::TransientTransaction(_))
            {
                tracing::error!(job_id = %job.base.id, account = %job.requested_by, "品牌导入暂未完成，将按原任务恢复");
            }
        }
        Ok(count)
    }
}

impl BrandImportProcess {
    /// 用 CAS 认领；每行执行前重新验证取消、提交人权限和输入。
    async fn execute(&self, job: &mut BackgroundJob) -> Result<()> {
        if !job.can_resume_import(Instant::now(), 120) {
            return Ok(());
        }
        if job.status == JobStatus::Pending {
            job.start(Instant::now())?;
        }
        job.last_progress_at = Some(Instant::now());
        self.db.background_jobs().update(job, &mut ReadExecutor::default()).await?;

        let request = match self.execution_source(job).await {
            Ok(request) => request,
            Err(_) => return self.fail_job(job, "导入源文件读取失败，请使用原文件重新提交核对").await,
        };
        let items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(&job.base.id), &mut ReadExecutor::default())
            .await?;

        let mut execution = BrandRows { process: self, request: &request };
        let result = execute_rows(&mut execution, job, items, import_budget()).await;

        if matches!(result, Err(Error::Forbidden(_) | Error::Unauthenticated(_))) {
            return self
                .fail_job(
                    job,
                    "提交人的账号或品牌创建权限已失效，任务已停止；请恢复权限后下载待处理行重新导入",
                )
                .await;
        }
        if job.finished_at.is_some() {
            self.sources.remove(&job.base.id).await;
        }
        result
    }

    /// 失败终态只保存概括信息，不覆盖并发取消或其他执行器的版本。
    async fn fail_job(&self, job: &mut BackgroundJob, message: &str) -> Result<()> {
        job.mark_failed(Some(message.into()), Instant::now())?;
        self.db.background_jobs().update(job, &mut ReadExecutor::default()).await?;
        self.sources.remove(&job.base.id).await;
        Ok(())
    }

    /// 来源只缓存不可变内容；任务状态与授权每行都从数据库重读。
    async fn execution_source(&self, job: &BackgroundJob) -> Result<Arc<BrandImportJobRequest>> {
        self.sources
            .get_or_load(job, || async {
                let request = tokio::time::timeout(Duration::from_secs(60), self.source(job))
                    .await
                    .map_err(|_| Error::Internal("读取品牌导入内容超时".into()))??;
                let bytes = serde_json::to_vec(&request)
                    .map_err(|_| Error::Internal("导入内容编码失败".into()))?
                    .len();
                Ok((request, bytes))
            })
            .await
    }

    /// 一个行事务持有全部写入；提交结果不确定时不自动重放，下一轮先读任务事实。
    async fn commit_row(
        &self,
        job: &mut BackgroundJob,
        item: BackgroundJobItem,
        request: &BrandImportJobRequest,
    ) -> Result<bool> {
        let db = self.db.clone();
        let process = self.clone();
        let expected = job.clone();
        let row_index = usize::try_from(item.item_no.saturating_sub(1))
            .map_err(|_| Error::Internal("导入行号无效".into()))?;
        let row =
            request.rows.get(row_index).cloned().ok_or_else(|| Error::Internal("导入源数据不完整".into()))?;
        let key = request.key()?;

        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let Some(mut current) = process.current_job(&expected, executor).await? else {
                        return Ok(None);
                    };
                    let mut item = db
                        .background_job_items()
                        .find_by_id(&item.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入行不存在".into()))?;
                    if item.status.is_some() {
                        return Err(Error::ConflictError("导入行已由其他执行器处理".into()));
                    }

                    let actor = job_actor(&current);
                    authorize(&process.authorization, &actor, executor).await?;
                    let outcome =
                        process.catalog.product_brand_import_row(&key, row, &actor, executor).await?;

                    record(&mut current, &mut item, outcome)?;
                    db.background_job_items().update(&mut item, executor).await?;
                    db.background_jobs().update(&mut current, executor).await?;
                    Ok::<_, Error>(Some(current))
                })
            })
            .await?;

        if let Some(updated) = result {
            *job = updated;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 决定写入前在同一事务中重读任务；取消和被接管都停止当前执行器。
    async fn current_job(
        &self,
        expected: &BackgroundJob,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        let current = self
            .db
            .background_jobs()
            .find_by_id(&expected.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
        if current.base.version != expected.base.version
            || current.finished_at.is_some()
            || !matches!(current.status, JobStatus::Running | JobStatus::PartiallySucceeded)
        {
            return Ok(None);
        }
        Ok(Some(current))
    }
}

struct BrandRows<'a> {
    process: &'a BrandImportProcess,
    request: &'a BrandImportJobRequest,
}
#[async_trait]
impl BrandRowExecution for BrandRows<'_> {
    async fn commit(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<bool> {
        self.process.commit_row(job, item, self.request).await
    }
    async fn release(&mut self, job: &mut BackgroundJob) -> Result<()> {
        if job.finished_at.is_none() {
            job.last_progress_at = None;
            self.process.db.background_jobs().update(job, &mut ReadExecutor::default()).await?;
        }
        Ok(())
    }
}
