//! 原提交人的待处理行下载；不通过公共文件 URL 暴露源数据。
use super::{SupplierImportProcess, SUPPLIER_IMPORT_DOMAIN};
use crate::{Error, Result};
use application_core::AuditActor;
use erp_supplier::dto::{import::SupplierImportResult, import_job::SupplierImportFailures};
use erp_support::{BackgroundJobId, BulkJobExt, ItemStatus};
use persistence_core::NoTransaction;

impl SupplierImportProcess {
    /// 返回任务中失败、待确认或停止后尚未执行的行，供修正重导。
    ///
    /// 仅原提交人可读取；任务执行中、无权限、密文损坏或仓储错误时拒绝。
    pub async fn failures(&self, id: &str, actor: &AuditActor) -> Result<SupplierImportFailures> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
        if job.requested_by != actor.id() || job.domain_job_type.as_deref() != Some(SUPPLIER_IMPORT_DOMAIN) {
            return Err(Error::Forbidden("仅原提交人可以下载供应商待处理行".into()));
        }
        if job.finished_at.is_none() {
            return Err(Error::ConflictError("请等待任务结束后下载待处理行".into()));
        }
        let source = self.source(&job).await?;
        let items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(id), &mut NoTransaction)
            .await?;
        let mut rows = Vec::new();
        let mut results = Vec::new();
        for item in items
            .into_iter()
            .filter(|i| !matches!(i.status, Some(ItemStatus::Success | ItemStatus::Skipped)))
        {
            let row = source
                .rows
                .get((item.item_no - 1) as usize)
                .ok_or_else(|| Error::Internal("导入源数据不完整".into()))?;
            results.push(SupplierImportResult {
                row_number: row.row_number,
                name: row.cell(1).into(),
                status: "failed".into(),
                message: item
                    .result_summary
                    .unwrap_or_else(|| "尚未执行，请重新导入".into()),
                supplier_id: None,
                supplier_no: None,
            });
            rows.push(row.clone());
        }
        Ok(SupplierImportFailures { rows, results })
    }
}
