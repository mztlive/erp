//! 登记导入文件资产并创建后台任务。

use application_core::AuditActor;
use erp_catalog::entity::catalog::product_import::{collapse_import_text, truncate_import_text};
use erp_catalog::{ProductImportJobView, PRODUCT_IMPORT_SHEET_NAME};
use erp_support::{
    product_import_job_no, BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData,
    BackgroundJobId, BackgroundJobItem, BackgroundJobItemDraft, BackgroundJobItemId,
    BackgroundJobRegistration, BulkJobExt, FileAsset, FileAssetExt, FileAssetId, JobType,
    RegisterFileAssetRequest, PRODUCT_IMPORT_DOMAIN_JOB_TYPE,
};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};

use super::parse::{parse_product_quote_xlsx, ParsedProductSheet};
use super::views::job_view;
use super::ProductImportProcess;
use crate::{Error, Result};

impl ProductImportProcess {
    /// 解析报价表并创建异步导入任务。
    ///
    /// # 参数
    /// * `file_name` - 原始文件名
    /// * `bytes` - 已写入对象存储前的文件内容（用于解析）
    /// * `registration` - 已写入对象存储的文件资产登记命令
    /// * `request_id` - 幂等请求身份
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回新建或幂等回放的导入任务。
    ///
    /// # 错误
    /// 文件不是原模板、没有数据行或写入失败时返回错误。
    pub async fn submit(
        &self,
        file_name: String,
        bytes: Vec<u8>,
        registration: RegisterFileAssetRequest,
        request_id: String,
        actor: &AuditActor,
    ) -> Result<ProductImportJobView> {
        let parsed = tokio::task::spawn_blocking(move || parse_product_quote_xlsx(&bytes))
            .await
            .map_err(|_| Error::Internal("解析导入文件失败".into()))??;
        let file_asset = FileAsset::new(FileAssetId::new(next_id()), registration.into_data(actor.id())?)?;
        self.create_job_from_parsed(file_asset, parsed, file_name, request_id, actor)
            .await
    }

    /// 由已解析工作表创建导入任务（表单上传与浏览器直传共用）。
    ///
    /// # 参数
    /// * `file_asset` - 已落对象存储的文件资产
    /// * `parsed` - 已解析报价表
    /// * `file_name` - 原始文件名
    /// * `request_id` - 幂等请求身份
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回新建或幂等回放的导入任务。
    pub(super) async fn create_job_from_parsed(
        &self,
        file_asset: FileAsset,
        parsed: ParsedProductSheet,
        file_name: String,
        request_id: String,
        actor: &AuditActor,
    ) -> Result<ProductImportJobView> {
        let job_id = BackgroundJobId::new(next_id());
        let drafts = parsed
            .rows
            .iter()
            .map(|row| {
                let name = row.cells.get(8).and_then(|value| {
                    let text = collapse_import_text(value);
                    if text.is_empty() {
                        None
                    } else {
                        Some(truncate_import_text(&text, 128))
                    }
                });
                BackgroundJobItemDraft {
                    id: BackgroundJobItemId::new(next_id()),
                    object_type: name.as_ref().map(|_| "product_import_row".to_string()),
                    object_id: name,
                    expected_version: None,
                    expected_hash: None,
                    worksheet_name: Some(PRODUCT_IMPORT_SHEET_NAME.to_string()),
                    source_row_no: Some(row.row_number),
                    source_column_name: None,
                }
            })
            .collect::<Vec<_>>();
        let total_rows = drafts.len() as u64;
        let aggregate = BackgroundJobAggregate::new(
            job_id,
            BackgroundJobAggregateData {
                job_no: product_import_job_no(&request_id),
                job_type: JobType::Import,
                domain_job_type: Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE.to_string()),
                domain_job_id: Some(file_asset.base.id.clone()),
                selection_snapshot_id: None,
                requested_by: actor.id().to_string(),
                request_id,
                input_file_asset_id: Some(FileAssetId::new(file_asset.base.id.clone())),
                result_file_asset_id: None,
                declared_total_count: total_rows,
            },
            drafts,
        )?;
        let (job, items) = aggregate.into_parts();
        self.persist_job(file_asset, job, items, file_name).await
    }

    pub(super) async fn persist_job(
        &self,
        file_asset: FileAsset,
        job: BackgroundJob,
        items: Vec<BackgroundJobItem>,
        file_name: String,
    ) -> Result<ProductImportJobView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let file_for_tx = file_asset.clone();
        let job_for_tx = job.clone();
        let registration = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.file_assets().create(&file_for_tx, session).await?;
                    db.bulk_job()
                        .create_job_with_items(&job_for_tx, items, session)
                        .await
                })
            })
            .await;
        match registration {
            Ok(BackgroundJobRegistration::Created) => Ok(job_view(&job, Some(file_name))),
            Ok(BackgroundJobRegistration::ReplaySame(existing)) => Ok(job_view(&existing, Some(file_name))),
            Ok(BackgroundJobRegistration::ConflictDifferentPayload(_)) => {
                Err(Error::ConflictError("同一请求身份已用于不同导入任务".into()))
            }
            Err(persistence_core::Error::DuplicateKey(_)) => self.replay_existing_job(&job.request_id).await,
            Err(error) => Err(error.into()),
        }
    }

    pub(super) async fn replay_existing_job(&self, request_id: &str) -> Result<ProductImportJobView> {
        let existing = self
            .db
            .background_jobs()
            .find_by_request_id(request_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("导入任务唯一竞争结果已变化，请刷新后重试".into()))?;
        Ok(job_view(&existing, None))
    }
}
