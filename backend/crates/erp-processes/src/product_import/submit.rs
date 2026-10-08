//! 登记导入文件资产并创建后台任务。

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::entity::catalog::product_import::{collapse_import_text, truncate_import_text};
use erp_catalog::{PRODUCT_IMPORT_SHEET_NAME, ProductImportJobView};
use erp_support::repository::bulk_job::BackgroundJobInput;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobId, BackgroundJobItem,
    BackgroundJobItemDraft, BackgroundJobItemId, BackgroundJobRegistration, BulkJobExt, FileAsset,
    FileAssetExt, FileAssetId, JobType, PRODUCT_IMPORT_DOMAIN_JOB_TYPE, RegisterFileAssetRequest,
    product_import_job_no,
};
use id_generator::next_id;
use persistence_core::{Error as PersistenceError, NoTransaction, Transactional};

use super::ProductImportProcess;
use super::parse::{ParsedProductSheet, parse_product_quote_xlsx};
use super::row_manifest::{build_row_manifest, delete_manifest_objects, prepare_row_manifest};
use super::views::job_view;
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
        let shared_bytes = Arc::new(bytes);
        let for_parse = shared_bytes.clone();
        let parsed = tokio::task::spawn_blocking(move || parse_product_quote_xlsx(&for_parse))
            .await
            .map_err(|_| Error::Internal("解析导入文件失败".into()))??;
        let file_asset = FileAsset::new(FileAssetId::new(next_id()), registration.into_data(actor.id())?)?;
        self.create_job_from_parsed(file_asset, parsed, file_name, request_id, actor, shared_bytes).await
    }

    /// 由已解析工作表创建导入任务（表单上传与浏览器直传共用）。
    ///
    /// 提交时预提各行图片；小清单与任务一起原子提交，较大清单仍存对象存储。
    ///
    /// # 参数
    /// * `file_asset` - 已落对象存储的文件资产
    /// * `parsed` - 已解析报价表
    /// * `file_name` - 原始文件名
    /// * `request_id` - 幂等请求身份
    /// * `actor` - 审计操作人
    /// * `xlsx` - 源文件字节（仅本次预提使用）
    ///
    /// # 返回
    /// 返回新建或幂等回放的导入任务。
    ///
    /// # 错误
    /// 任务装配、清单构建、清单写入或任务登记失败时返回对应错误。
    /// 清单或登记失败会删除本次已上传对象；`OutcomeUnknown` 不执行对象补偿。
    pub(super) async fn create_job_from_parsed(
        &self,
        file_asset: FileAsset,
        parsed: ParsedProductSheet,
        file_name: String,
        request_id: String,
        actor: &AuditActor,
        xlsx: Arc<Vec<u8>>,
    ) -> Result<ProductImportJobView> {
        let (job, items) = import_job(&parsed, &file_asset, &request_id, actor)?;
        let built = build_row_manifest(&self.storage, &self.secret, &request_id, &parsed, xlsx).await?;
        let (input, manifest_key) = match prepare_row_manifest(&self.storage, &built.manifest).await {
            Ok(prepared) => prepared,
            Err(error) => {
                delete_manifest_objects(&self.storage, &built.uploaded_object_keys).await;
                return Err(error);
            },
        };
        let mut manifest_keys = built.uploaded_object_keys;
        manifest_keys.extend(manifest_key);
        match self.persist_job(file_asset, job, items, file_name, input).await {
            Ok(view) => Ok(view),
            Err(error) => {
                if !matches!(error, Error::OutcomeUnknown(_)) {
                    delete_manifest_objects(&self.storage, &manifest_keys).await;
                }
                Err(error)
            },
        }
    }

    /// 原子登记源文件资产、任务、逐行结果表和可选私有清单。
    ///
    /// # 参数
    /// * `file_asset` - 已落对象存储的文件资产。
    /// * `job` - 待登记的导入任务。
    /// * `items` - 逐行明细。
    /// * `file_name` - 原始文件名。
    /// * `input` - 已通过容量校验的可选私有清单。
    ///
    /// # 返回
    /// 返回新任务或既有幂等回放任务。
    /// # 错误
    /// 事务、唯一键或异载荷冲突按原分类传播；未知提交不执行对象补偿。
    pub(super) async fn persist_job(
        &self,
        file_asset: FileAsset,
        job: BackgroundJob,
        items: Vec<BackgroundJobItem>,
        file_name: String,
        input: Option<BackgroundJobInput>,
    ) -> Result<ProductImportJobView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let file_for_tx = file_asset.clone();
        let job_for_tx = job.clone();
        let registration = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.file_assets().create(&file_for_tx, executor).await?;
                    let registration =
                        db.bulk_job().create_job_with_items(&job_for_tx, items, executor).await?;
                    if let Some(input) = input {
                        let job_id = BackgroundJobId::new(job_for_tx.base.id.clone());
                        db.bulk_job().create_job_input(&job_id, &input, executor).await?;
                    }
                    Ok::<_, PersistenceError>(registration)
                })
            })
            .await;
        match registration {
            Ok(BackgroundJobRegistration::Created) => Ok(job_view(&job, Some(file_name))),
            Ok(BackgroundJobRegistration::ReplaySame(existing)) => Ok(job_view(&existing, Some(file_name))),
            Ok(BackgroundJobRegistration::ConflictDifferentPayload(_)) => {
                Err(Error::ConflictError("同一请求身份已用于不同导入任务".into()))
            },
            Err(PersistenceError::DuplicateKey(_)) => self.replay_existing_job(&job.request_id).await,
            Err(error) => Err(error.into()),
        }
    }

    /// 按请求身份回放已存在的导入任务。
    ///
    /// # 参数
    /// * `request_id` - 幂等请求身份。
    ///
    /// # 返回
    /// 返回已存在任务的视图；文件名不随回放恢复。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。找不到任务时返回 `ConflictError`。
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

/// 用原模板行顺序构造任务明细，保留行号、名称与独立明细身份。
fn import_item_drafts(parsed: &ParsedProductSheet) -> Vec<BackgroundJobItemDraft> {
    parsed
        .rows
        .iter()
        .map(|row| {
            let name = row.cells.get(8).and_then(|value| {
                let text = collapse_import_text(value);
                if text.is_empty() { None } else { Some(truncate_import_text(&text, 128)) }
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
        .collect()
}

/// 由原领域聚合构造任务及明细，业务校验仍由后台任务实体执行。
fn import_job(
    parsed: &ParsedProductSheet,
    file_asset: &FileAsset,
    request_id: &str,
    actor: &AuditActor,
) -> Result<(BackgroundJob, Vec<BackgroundJobItem>)> {
    let job_id = BackgroundJobId::new(next_id());
    let drafts = import_item_drafts(parsed);
    let total_rows = u64::try_from(drafts.len()).expect("导入行数不超过 1000");
    let aggregate = BackgroundJobAggregate::new(
        job_id,
        BackgroundJobAggregateData {
            job_no: product_import_job_no(request_id),
            job_type: JobType::Import,
            domain_job_type: Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE.to_string()),
            domain_job_id: Some(file_asset.base.id.clone()),
            selection_snapshot_id: None,
            requested_by: actor.id().to_string(),
            request_id: request_id.to_string(),
            input_file_asset_id: Some(FileAssetId::new(file_asset.base.id.clone())),
            result_file_asset_id: None,
            declared_total_count: total_rows,
        },
        drafts,
    )?;
    Ok(aggregate.into_parts())
}
