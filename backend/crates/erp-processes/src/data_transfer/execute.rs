use std::time::Duration;

use erp_core::common::time::Instant;
use erp_core::ids::BackgroundJobId;
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BulkJobExt, FileAssetExt, JobUpdate};
use futures_util::{StreamExt, stream};
use persistence_core::{NoTransaction, Transactional};
use tokio::time::{MissedTickBehavior, interval};

use super::outcome::apply as apply_outcome;
use super::submit::DOMAIN;
use super::{DataTransferProcess, TransferResult};
use crate::{Error, Result};

impl DataTransferProcess {
    /// 独立执行循环每轮最多读取八条、并发执行两条，不阻塞其他业务 worker。
    /// # 参数
    /// 无。
    /// # 返回
    /// 扫描到的任务数。
    /// # 错误
    /// 查询失败返回错误；单任务失败记录状态或等待执行权恢复。
    pub async fn run_due(&self) -> Result<usize> {
        let jobs = self
            .db
            .background_jobs()
            .claimable_jobs(DOMAIN, Instant::now().unix_secs() - 120, 8, &mut NoTransaction)
            .await?;
        let count = jobs.len();
        stream::iter(jobs)
            .for_each_concurrent(2, |job| async move {
                if self.execute_job(job).await.is_err() {
                    tracing::warn!(event = "data_transfer_recover", "任务未确认完成，保留原任务等待恢复");
                }
            })
            .await;
        Ok(count)
    }
    async fn execute_job(&self, mut job: BackgroundJob) -> Result<()> {
        if !job.claim_execution(Instant::now())? {
            return Ok(());
        }
        self.db.background_jobs().update(&mut job, &mut NoTransaction).await?;
        let result = self.execute_with_heartbeat(&mut job).await;
        match result {
            Ok(result) => self.complete(job, result).await,

            Err(error) => {
                job.mark_failed(Some(safe_failure(&error)), Instant::now())?;
                self.db.background_jobs().update(&mut job, &mut NoTransaction).await?;
                Ok(())
            },
        }
    }
    async fn execute_with_heartbeat(&self, job: &mut BackgroundJob) -> Result<TransferResult> {
        let source = self.source(job).await?;
        let actor = source.actor(&job.requested_by, &job.request_id)?;
        let work = self.handler.execute(source.kind, &source.input, &actor);
        tokio::pin!(work);
        let mut heartbeat = interval(Duration::from_secs(20));
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut owned = true;
        loop {
            tokio::select! {
                result = &mut work => {
                    if !owned { return Err(Error::ConflictError("任务已取消或由其他执行器接管".into())); }
                    return result;
                }
                _ = heartbeat.tick(), if owned => {
                    owned = self.heartbeat(job).await.is_ok();
                    // 写命令必须等待原事务与回执收尾，不以取消 future 方式重放。
                    if !owned && source.kind.is_export() {
                        return Err(Error::ConflictError("任务已取消或由其他执行器接管".into()));
                    }
                }
            }
        }
    }
    async fn heartbeat(&self, job: &mut BackgroundJob) -> Result<()> {
        job.heartbeat_execution(Instant::now())?;
        self.db.background_jobs().update(job, &mut NoTransaction).await?;
        Ok(())
    }
    async fn complete(&self, mut job: BackgroundJob, result: TransferResult) -> Result<()> {
        self.heartbeat(&mut job).await?;
        let key = format!("data-transfer/{}/result-{}.enc", job.base.id, job.base.version);
        let mut asset = self.save_private(&key, &result, &job.requested_by).await?;
        let source_expiry = self.source_expiry(&job).await?;
        asset.expires_at = asset.expires_at.map(|expires| expires.min(source_expiry));
        self.heartbeat(&mut job).await?;
        let mut items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(&job.base.id), &mut NoTransaction)
            .await?;
        apply_outcome(&mut job, &mut items, &result)?;
        job.update(JobUpdate {
            result_file_asset_id: Some(asset.base.id.clone().into()),
            result_expires_at: asset.expires_at,
        });
        let db = self.db.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.file_assets().create(&asset, executor).await?;
                    for item in &mut items {
                        db.background_job_items().update(item, executor).await?;
                    }
                    db.background_jobs().update(&mut job, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await
    }
}
fn safe_failure(error: &Error) -> String {
    match error {
        Error::Forbidden(_) | Error::Unauthenticated(_) => {
            "TRANSFER_FORBIDDEN：当前权限或业务范围已变化，请重新查询后提交".into()
        },
        Error::ValidationError(message)
        | Error::BusinessLogicError(message)
        | Error::ConflictError(message) => {
            format!("TRANSFER_INVALID：{}", message.chars().take(400).collect::<String>())
        },
        Error::InvalidInput(_) => "TRANSFER_INVALID：字段校验未通过，请返回业务页面核对输入".into(),
        Error::OutcomeUnknown(_) => {
            "TRANSFER_OUTCOME_UNKNOWN：提交结果待核对，请使用原操作号恢复，勿重复登记".into()
        },
        _ => "TRANSFER_FAILED：处理未完成，请查询原任务；重试写入须沿用原操作号".into(),
    }
}
