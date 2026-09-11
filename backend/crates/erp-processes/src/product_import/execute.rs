//! 领取并执行商品导入后台任务。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::BackgroundJobId;
use erp_core::AccountKind;
use erp_support::{BulkJobExt, FileAssetExt, ItemStatus, JobStatus, PRODUCT_IMPORT_DOMAIN_JOB_TYPE};
use persistence_core::{NoTransaction, Transactional};

use super::parse::parse_product_quote_xlsx;
use super::resolve::ImportDictionaryCache;
use super::ProductImportProcess;
use crate::{Error, Result};

impl ProductImportProcess {
    /// 领取待处理的商品导入任务并逐行执行。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本轮处理的任务数。
    ///
    /// # 错误
    /// 查询失败时返回错误；单个任务失败记日志后继续。
    pub async fn run_due_jobs(&self) -> Result<usize> {
        let jobs = self
            .db
            .background_jobs()
            .list_open_by_domain_job_type(PRODUCT_IMPORT_DOMAIN_JOB_TYPE, 4, &mut NoTransaction)
            .await?;
        let count = jobs.len();
        for job in jobs {
            if let Err(error) = self.execute_job(&job.base.id).await {
                tracing::error!(job_id = %job.base.id, error = %error, "商品导入任务执行失败");
            }
        }
        Ok(count)
    }

    /// 执行单个导入任务中尚未完成的行。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 源文件不存在、解析失败或状态迁移失败时返回错误。
    pub async fn execute_job(&self, job_id: &str) -> Result<()> {
        let mut job = self
            .db
            .background_jobs()
            .find_by_id(job_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
        if job.domain_job_type.as_deref() != Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE) {
            return Ok(());
        }
        if job.status == JobStatus::Pending {
            job.start(Instant::now())?;
            self.db
                .background_jobs()
                .update(&mut job, &mut NoTransaction)
                .await?;
        }
        if !matches!(job.status, JobStatus::Running | JobStatus::PartiallySucceeded) {
            return Ok(());
        }
        let (bytes, parsed) = self.load_source(&job).await?;
        let items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(job.base.id.clone()), &mut NoTransaction)
            .await?;
        let actor = AuditActor::new(
            job.requested_by.clone(),
            job.requested_by.clone(),
            AccountKind::Admin,
        );
        let mut cache = ImportDictionaryCache::default();
        let remaining = items.iter().filter(|item| item.status.is_none()).count();
        if remaining == 0 {
            if job.processed_count == job.total_count
                && matches!(job.status, JobStatus::Running | JobStatus::PartiallySucceeded)
            {
                job.record_import_result_batch(0, 0, 0, true, Instant::now())?;
                self.db
                    .background_jobs()
                    .update(&mut job, &mut NoTransaction)
                    .await?;
            }
            return Ok(());
        }
        let mut done = 0usize;
        for mut item in items {
            if item.status.is_some() {
                continue;
            }
            let row_number = item.source_row_no.unwrap_or(item.item_no);
            let cells = parsed
                .rows
                .iter()
                .find(|row| row.row_number == row_number)
                .map(|row| row.cells.as_slice())
                .unwrap_or(&[]);
            let recorded = record_row_outcome(
                self.import_row(row_number, cells, &bytes, &parsed, &actor, &mut cache)
                    .await,
            );
            item.record_result(
                recorded.status,
                recorded.code,
                recorded.summary,
                recorded.object_type,
                recorded.object_id,
            )?;
            done += 1;
            let success = u64::from(recorded.status == ItemStatus::Success);
            let skipped = u64::from(recorded.status == ItemStatus::Skipped);
            let failed = u64::from(recorded.status == ItemStatus::Failed);
            job.record_import_result_batch(success, skipped, failed, done == remaining, Instant::now())?;
            self.persist_progress(&mut job, item).await?;
        }
        Ok(())
    }

    async fn load_source(
        &self,
        job: &erp_support::BackgroundJob,
    ) -> Result<(Vec<u8>, super::parse::ParsedProductSheet)> {
        let file_id = job
            .input_file_asset_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("导入任务缺少源文件".into()))?;
        let asset = self
            .db
            .file_assets()
            .find_by_id(file_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入源文件不存在".into()))?;
        let bytes = self
            .storage
            .read(&asset.storage_object_key)
            .await
            .map_err(|error| Error::Internal(format!("读取导入文件失败: {error}")))?;
        let parse_bytes = bytes.clone();
        let parsed = tokio::task::spawn_blocking(move || parse_product_quote_xlsx(&parse_bytes))
            .await
            .map_err(|_| Error::Internal("解析导入文件失败".into()))??;
        Ok((bytes, parsed))
    }

    async fn persist_progress(
        &self,
        job: &mut erp_support::BackgroundJob,
        mut item: erp_support::BackgroundJobItem,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut job_for_tx = job.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.background_job_items().update(&mut item, session).await?;
                    db.background_jobs().update(&mut job_for_tx, session).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await?;
        let latest = self
            .db
            .background_jobs()
            .find_by_id(&job.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
        *job = latest;
        Ok(())
    }
}

struct RecordedOutcome {
    status: ItemStatus,
    code: Option<String>,
    summary: Option<String>,
    object_type: Option<String>,
    object_id: Option<String>,
}

fn record_row_outcome(outcome: Result<super::row::RowImportOutcome>) -> RecordedOutcome {
    match outcome {
        Ok(result) if result.skipped => {
            recorded(ItemStatus::Skipped, None, result.message, result.product_id)
        }
        Ok(result) => recorded(ItemStatus::Success, None, result.message, result.product_id),
        Err(error) => recorded(ItemStatus::Failed, Some("import_failed"), error.to_string(), None),
    }
}

fn recorded(
    status: ItemStatus,
    code: Option<&str>,
    summary: String,
    product_id: Option<String>,
) -> RecordedOutcome {
    let (object_type, object_id) = match product_id {
        Some(id) => (Some("product".to_string()), Some(id)),
        None => (None, None),
    };
    RecordedOutcome {
        status,
        code: code.map(str::to_string),
        summary: Some(summary),
        object_type,
        object_id,
    }
}
