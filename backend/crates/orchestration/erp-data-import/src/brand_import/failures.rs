//! 原提交人的待处理行下载；不通过公共文件 URL 暴露源数据。
use application_core::AuditActor;
use erp_catalog::dto::{BrandImportFailures, BrandImportJobRequest, BrandImportResult};
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BackgroundJobId, BackgroundJobItem, BulkJobExt, ItemStatus};
use persistence_core::Transactional;

use super::BrandImportProcess;
use crate::{Error, Result};

impl BrandImportProcess {
    /// 返回任务中失败、待确认或停止后尚未执行的行，供修正重导。
    ///
    /// `id`：导入任务主键；`actor`：当前操作人，必须是原提交人。
    ///
    /// 返回尚未成功或跳过的源行，以及对应的失败说明。
    ///
    /// 任务不存在、不是原提交人或不是品牌导入、任务尚未结束、源数据不可读或不完整，或仓储失败时返回错误。
    pub async fn failures(&self, id: &str, actor: &AuditActor) -> Result<BrandImportFailures> {
        let job = self.job_access.snapshot(id, actor).await?;
        ensure_download(&job)?;
        let source = self.source(&job).await?;

        let db = self.db.clone();
        let access = self.job_access.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let current = access.reload(&job, &actor, executor).await?;
                    ensure_download(&current)?;
                    let items = db
                        .background_job_items()
                        .list_entities_by_job(&BackgroundJobId::new(&current.base.id), executor)
                        .await?;
                    failure_rows(&source, items)
                })
            })
            .await
    }
}

// identity 已证明本人和任务类型，业务继续要求任务已结束。
fn ensure_download(job: &BackgroundJob) -> Result<()> {
    if job.finished_at.is_none() {
        return Err(Error::ConflictError("请等待任务结束后下载待处理行".into()));
    }
    Ok(())
}

// 跨域映射仅保留失败与尚未执行的原始行。
fn failure_rows(
    source: &BrandImportJobRequest,
    items: Vec<BackgroundJobItem>,
) -> Result<BrandImportFailures> {
    let mut rows = Vec::new();
    let mut results = Vec::new();
    for item in
        items.into_iter().filter(|i| !matches!(i.status, Some(ItemStatus::Success | ItemStatus::Skipped)))
    {
        let row = source
            .rows
            .get(
                usize::try_from(item.item_no.saturating_sub(1))
                    .map_err(|_| Error::Internal("导入行号无效".into()))?,
            )
            .ok_or_else(|| Error::Internal("导入源数据不完整".into()))?;
        results.push(BrandImportResult::failed(
            row.row_number,
            item.result_summary.unwrap_or_else(|| "尚未执行，请重新导入".into()),
        ));
        rows.push(row.clone());
    }
    Ok(BrandImportFailures { rows, results })
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::*;
    use crate::brand_import::submit::tests::{fixture, request};
    #[test]
    fn download_requires_terminal_state_after_identity_authorization() {
        let (mut job, _) = fixture();
        assert!(matches!(ensure_download(&job), Err(Error::ConflictError(_))));
        job.cancel(Instant::now()).unwrap();
        assert!(ensure_download(&job).is_ok());
    }
    #[test]
    fn failure_download_keeps_unprocessed_and_failed_rows_only() {
        let (_, items) = fixture();
        for status in [None, Some(ItemStatus::Success), Some(ItemStatus::Skipped), Some(ItemStatus::Failed)] {
            let mut item = items[0].clone();
            item.status = status;
            item.result_summary = Some("需要核对".into());
            let output = failure_rows(&request(), vec![item]).unwrap();
            let expected = !matches!(status, Some(ItemStatus::Success | ItemStatus::Skipped));
            assert_eq!(output.rows.len(), usize::from(expected));
            if expected {
                assert_eq!(output.rows[0].row_number, 9);
                assert_eq!(output.results[0].message, "需要核对");
            }
        }
    }
}
