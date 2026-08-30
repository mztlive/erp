use database::{AccessControlExt, BulkJobExt, FileAssetExt, LegacyImportExt, NoTransaction, Transactional};
use entities::legacy_import::{
    LegacyImportBatch, LegacyImportBatchId, LegacyImportBatchStatus, LegacyImportRow, LegacyImportRowData,
    LegacyImportRowId,
};
use id_generator::next_id;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{build_background_job, CreateLegacyImportBatchRequest, LegacyImportBatchView};
use super::LegacyImportService;

impl LegacyImportService {
    /// 创建导入批次（批次 + 来源行 + 后台任务原子写入）。
    ///
    /// 批次号唯一：重复提交按幂等处理，直接返回既有批次（不产生重复事实）。
    /// 资产引用（成功包/manifest/失败诊断包）存在性经 D05 仓储校验，
    /// 后台任务经 D04 仓储与批次同一事务登记。
    ///
    /// # 参数
    /// * `req` - 创建请求（批次头 + 来源行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或既有）批次的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 资产引用不存在
    /// * `ValidationError` - 请求体校验失败
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_batch(
        &self,
        req: CreateLegacyImportBatchRequest,
        actor: &AuditActor,
    ) -> Result<LegacyImportBatchView> {
        req.validate()?;
        if let Some(existing) = self
            .db
            .legacy_import_batches()
            .find_by_batch_no(&req.batch_no, &mut NoTransaction)
            .await?
        {
            tracing::info!(batch_no = %req.batch_no, "批次已存在，按幂等返回既有批次");
            return self.batch_view_of(existing).await;
        }
        self.ensure_file_assets_exist(&req).await?;

        let id = LegacyImportBatchId::new(next_id());
        let rows = self.build_rows(&req, &id)?;
        let batch = LegacyImportBatch::new(
            id.clone(),
            entities::legacy_import::LegacyImportBatchData {
                batch_no: req.batch_no,
                source_system_id: req.source_system_id,
                source_object_set: req.source_object_set,
                baseline_date: req.baseline_date,
                import_rule_version: req.import_rule_version,
                source_file_hmac: req.source_file_hmac,
                status: LegacyImportBatchStatus::PendingValidation,
                total_rows: rows.len() as u64,
                success_rows: 0,
                failed_rows: 0,
                failure_code_summary: None,
                confirmation_status_summary: None,
            },
        )?;
        let background_job = build_background_job(&batch, actor.id())?;
        let audit = actor.clone().resource_log(
            "legacy_import_batch.create",
            "legacy_import_batch",
            id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let batch_for_tx = batch.clone();
        let rows_for_tx = rows.clone();
        let job_for_tx = background_job.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.legacy_import()
                        .create_batch_with_rows(&batch_for_tx, &rows_for_tx, session)
                        .await?;
                    db.background_jobs().create(&job_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        let mut view: LegacyImportBatchView = batch.into();
        view.background_job_id = Some(background_job.base.id);
        Ok(view)
    }

    /// 校验批次引用的资产存在（D05 仓储读取）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    ///
    /// # 错误
    /// * `NotFound` - 资产引用不存在
    async fn ensure_file_assets_exist(&self, req: &CreateLegacyImportBatchRequest) -> Result<()> {
        for (label, asset_id) in [
            ("成功白名单包", req.successful_sanitized_file_asset_id.as_ref()),
            ("成功 manifest", req.success_manifest_file_asset_id.as_ref()),
            ("失败诊断包", req.failure_diagnostic_file_asset_id.as_ref()),
        ] {
            if let Some(asset_id) = asset_id {
                if self
                    .db
                    .file_assets()
                    .find_by_id(asset_id.as_ref(), &mut NoTransaction)
                    .await?
                    .is_none()
                {
                    return Err(Error::NotFound(format!("{label}资产不存在")));
                }
            }
        }
        Ok(())
    }

    /// 构造导入行实体列表。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `batch_id` - 所属导入批次
    ///
    /// # 返回
    /// 返回新建的导入行实体列表。
    ///
    /// # 错误
    /// 行字段校验失败时返回错误。
    fn build_rows(
        &self,
        req: &CreateLegacyImportBatchRequest,
        batch_id: &LegacyImportBatchId,
    ) -> Result<Vec<LegacyImportRow>> {
        req.rows
            .iter()
            .map(|row| {
                LegacyImportRow::new(
                    LegacyImportRowId::new(next_id()),
                    LegacyImportRowData {
                        batch_id: batch_id.clone(),
                        source_object_type: row.source_object_type.clone(),
                        source_row_key: row.source_row_key.clone(),
                        normalized_payload_reference: row.normalized_payload_reference.clone(),
                    },
                )
                .map_err(Into::into)
            })
            .collect()
    }
}
