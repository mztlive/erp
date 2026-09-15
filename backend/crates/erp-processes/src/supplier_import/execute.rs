//! 有界认领、逐行幂等执行与进度原子更新。
use std::time::Duration;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_supplier::dto::import::{SupplierImportRequest, SupplierImportResult};
use erp_supplier::dto::import_job::SupplierImportJobRequest;
use erp_support::{BackgroundJob, BackgroundJobId, BackgroundJobItem, BulkJobExt, ItemStatus, JobStatus};
use persistence_core::{NoTransaction, Transactional};

use super::{SUPPLIER_IMPORT_DOMAIN, SupplierImportProcess};
use crate::{Error, Result, SupplierProfileService};

impl SupplierImportProcess {
    /// 处理至多四个未完成任务，恢复超过两分钟未推进的任务。
    ///
    /// 返回扫描任务数；查询失败返回错误，单任务失败留待下轮恢复并记录脱敏日志。
    pub async fn run_due_jobs(&self) -> Result<usize> {
        let jobs = self
            .db
            .background_jobs()
            .list_open_by_domain_job_type(SUPPLIER_IMPORT_DOMAIN, 4, &mut NoTransaction)
            .await?;
        let count = jobs.len();
        for mut job in jobs {
            if let Err(error) = self.execute(&mut job).await {
                if matches!(error, Error::ConflictError(_)) {
                    continue;
                }
                tracing::error!(job_id = %job.base.id, "供应商导入任务暂未完成，下一轮恢复");
            }
        }
        Ok(count)
    }

    /// 通过版本竞争认领任务，执行前加载密文；读取失败终止任务并提示重新提交。
    async fn execute(&self, job: &mut BackgroundJob) -> Result<()> {
        if !claimable(job, Instant::now()) {
            return Ok(());
        }
        if job.status == JobStatus::Pending {
            job.start(Instant::now())?;
        }
        job.last_progress_at = Some(Instant::now());
        self.db.background_jobs().update(job, &mut NoTransaction).await?;
        let request = match tokio::time::timeout(Duration::from_secs(60), self.source(job)).await {
            Ok(Ok(request)) => request,
            _ => {
                job.mark_failed(Some("导入源文件读取失败，请使用原文件重新提交核对".into()), Instant::now())?;
                self.db.background_jobs().update(job, &mut NoTransaction).await?;
                return Ok(());
            },
        };
        self.execute_rows(job, request).await
    }

    /// 每行最多执行一分钟；异常结果单独标识为待核对，其余行继续处理。
    async fn execute_rows(&self, job: &mut BackgroundJob, request: SupplierImportJobRequest) -> Result<()> {
        let items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(&job.base.id), &mut NoTransaction)
            .await?;
        let service = SupplierProfileService::new(self.db.clone(), self.codec.clone());
        let actor = AuditActor::new(job.requested_by.clone(), job.requested_by.clone(), AccountKind::Admin);
        for mut item in items.into_iter().filter(|item| item.status.is_none()) {
            if !self.renew(job).await? {
                return Ok(());
            }
            let outcome = self.import_item(&service, &actor, &request, &item).await;
            record(&mut item, outcome)?;
            let status = item.status.expect("record always sets status");
            job.record_import_result_batch(
                u64::from(status == ItemStatus::Success),
                u64::from(status == ItemStatus::Skipped),
                u64::from(status == ItemStatus::Failed),
                job.processed_count + 1 == job.total_count,
                Instant::now(),
            )?;
            self.persist_progress(job, item).await?;
        }
        Ok(())
    }

    /// 逐行开始前重验任务版本和取消状态，再续期认领。
    async fn renew(&self, job: &mut BackgroundJob) -> Result<bool> {
        let current = self
            .db
            .background_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
        if current.base.version != job.base.version || current.finished_at.is_some() {
            return Ok(false);
        }
        job.last_progress_at = Some(Instant::now());
        self.db.background_jobs().update(job, &mut NoTransaction).await?;
        Ok(true)
    }

    /// 有界执行单行；超时或未知响应返回待确认，不推断是否已经写入。
    async fn import_item(
        &self,
        service: &SupplierProfileService,
        actor: &AuditActor,
        request: &SupplierImportJobRequest,
        item: &BackgroundJobItem,
    ) -> Option<SupplierImportResult> {
        let row = request.rows.get((item.item_no - 1) as usize)?.clone();
        tokio::time::timeout(
            Duration::from_secs(60),
            service.import(SupplierImportRequest { rows: vec![row] }, actor),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .and_then(|rows| rows.into_iter().next())
    }

    /// 进度与行结果同时提交；取消或另一执行器更新任务后版本冲突会回滚本次进度。
    async fn persist_progress(&self, job: &mut BackgroundJob, mut item: BackgroundJobItem) -> Result<()> {
        let db = self.db.clone();
        let mut copy = job.clone();
        let updated = db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.background_job_items().update(&mut item, session).await?;
                    db.background_jobs().update(&mut copy, session).await?;
                    Ok::<_, Error>(copy)
                })
            })
            .await?;
        *job = updated;
        Ok(())
    }

    /// 仅 worker 和已校验归属的失败下载调用；解密失败不泄漏源内容。
    pub(super) async fn source(&self, job: &BackgroundJob) -> Result<SupplierImportJobRequest> {
        let source = job
            .domain_job_id
            .as_deref()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| Error::Internal("导入任务缺少源数据".into()))?;
        let bytes = self
            .storage
            .read(super::submit::source_key(source))
            .await
            .map_err(|_| Error::Internal("读取供应商导入源文件失败".into()))?;
        let encoded = std::str::from_utf8(&bytes).map_err(|_| Error::Internal("导入源数据损坏".into()))?;
        let json = self.codec.decrypt(encoded)?;
        let request: SupplierImportJobRequest =
            serde_json::from_str(&json).map_err(|_| Error::Internal("导入源数据损坏".into()))?;
        request.validate()?;
        let expected = application_core::command::CommandFingerprint::from_parts([
            hex::encode(self.codec.fingerprint_key()),
            job.requested_by.clone(),
            json,
        ]);
        if expected.digest_hex() != source {
            return Err(Error::Forbidden("导入源数据不属于该任务".into()));
        }
        let (proposed, _) = super::submit::build_job(&request, &job.requested_by, source)?;
        super::submit::ensure_replay(job, &proposed)?;
        Ok(request)
    }
}

/// 最近仍在推进的任务不得抢占；终态包括已经完成的部分成功。
fn claimable(job: &BackgroundJob, now: Instant) -> bool {
    if job.finished_at.is_some() {
        return false;
    }
    job.status == JobStatus::Pending
        || (matches!(job.status, JobStatus::Running | JobStatus::PartiallySucceeded)
            && job.last_progress_at.is_none_or(|last| now.unix_secs() - last.unix_secs() >= 120))
}

/// 未知结果使用专属原因码，不声明回滚；失败项可由原文件重导核对。
fn record(item: &mut BackgroundJobItem, outcome: Option<SupplierImportResult>) -> Result<()> {
    let Some(result) = outcome.filter(|row| row.status != "uncertain") else {
        return Ok(item.record_result(
            ItemStatus::Failed,
            Some("outcome_unknown".into()),
            Some("结果待确认，请下载待处理行后重新导入核对；已导入资料不会重复创建".into()),
            None,
            None,
        )?);
    };
    let status = match result.status.as_str() {
        "succeeded" => ItemStatus::Success,
        "skipped" => ItemStatus::Skipped,
        _ => ItemStatus::Failed,
    };
    Ok(item.record_result(
        status,
        (status == ItemStatus::Failed).then(|| "import_failed".into()),
        Some(result.message.chars().take(160).collect()),
        result.supplier_id.as_ref().map(|_| "supplier".into()),
        result.supplier_id,
    )?)
}

#[cfg(test)]
mod tests {
    use super::super::submit::tests::fixture;
    use super::*;
    #[test]
    fn claim_respects_active_lease_recovery_and_completed_partial_jobs() {
        let (mut job, _) = fixture();
        let at = Instant::from_unix_secs(1000);
        assert!(claimable(&job, at));
        job.start(at).unwrap();
        assert!(!claimable(&job, Instant::from_unix_secs(1119)));
        assert!(claimable(&job, Instant::from_unix_secs(1120)));
        job.status = JobStatus::PartiallySucceeded;
        job.finished_at = Some(at);
        assert!(!claimable(&job, Instant::from_unix_secs(2000)));
        job.status = JobStatus::Cancelled;
        assert!(!claimable(&job, Instant::from_unix_secs(2000)));
    }
    #[test]
    fn records_created_skipped_failed_and_unknown_without_overwriting() {
        for (status, expected) in [
            ("succeeded", ItemStatus::Success),
            ("skipped", ItemStatus::Skipped),
            ("failed", ItemStatus::Failed),
            ("uncertain", ItemStatus::Failed),
        ] {
            let (_, mut items) = fixture();
            let item = &mut items[0];
            record(
                item,
                Some(SupplierImportResult {
                    row_number: 9,
                    name: "供应商".into(),
                    status: status.into(),
                    message: "处理结果".into(),
                    supplier_id: None,
                    supplier_no: None,
                }),
            )
            .unwrap();
            assert_eq!(item.status, Some(expected));
            if status == "uncertain" {
                assert_eq!(item.result_code.as_deref(), Some("outcome_unknown"));
            }
            assert!(record(item, None).is_err());
        }
        let (_, mut items) = fixture();
        record(&mut items[0], None).unwrap();
        assert_eq!(items[0].result_code.as_deref(), Some("outcome_unknown"));
    }
}
