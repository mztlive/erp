//! 原提交人的待处理行下载；不通过公共文件 URL 暴露源数据。
use application_core::AuditActor;
use erp_supplier::dto::import::SupplierImportResult;
use erp_supplier::dto::import_job::{SupplierImportFailures, SupplierImportJobRequest};
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BackgroundJobId, BackgroundJobItem, BulkJobExt, ItemStatus};
use persistence_core::Transactional;

use super::SupplierImportProcess;
use crate::{Error, Result};

impl SupplierImportProcess {
    /// 返回任务中失败、待确认或停止后尚未执行的行，供修正重导。
    ///
    /// `id`：导入任务主键；`actor`：当前操作人，必须是原提交人。
    ///
    /// 返回尚未成功或跳过的源行，以及对应的失败说明。
    ///
    /// 任务不存在、不是原提交人或不是供应商导入、任务尚未结束、源数据不可读或不完整，或仓储失败时返回错误。
    pub async fn failures(&self, id: &str, actor: &AuditActor) -> Result<SupplierImportFailures> {
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

// 已授权任务仅在终态允许下载。
fn ensure_download(job: &BackgroundJob) -> Result<()> {
    if job.finished_at.is_none() {
        return Err(Error::ConflictError("请等待任务结束后下载待处理行".into()));
    }
    Ok(())
}

// 仅输出尚未成功或跳过的源行。
fn failure_rows(
    source: &SupplierImportJobRequest,
    items: Vec<BackgroundJobItem>,
) -> Result<SupplierImportFailures> {
    let mut rows = Vec::new();
    let mut results = Vec::new();
    for item in
        items.into_iter().filter(|i| !matches!(i.status, Some(ItemStatus::Success | ItemStatus::Skipped)))
    {
        let row = source
            .rows
            .get((item.item_no - 1) as usize)
            .ok_or_else(|| Error::Internal("导入源数据不完整".into()))?;
        results.push(SupplierImportResult {
            row_number: row.row_number,
            name: row.cell(1).into(),
            status: "failed".into(),
            message: item.result_summary.unwrap_or_else(|| "尚未执行，请重新导入".into()),
            supplier_id: None,
            supplier_no: None,
        });
        rows.push(row.clone());
    }
    Ok(SupplierImportFailures { rows, results })
}
