use database::{AccessControlExt, BulkJobExt, FileAssetExt, LegacyImportExt, NoTransaction, Transactional};
use entities::bulk_job::BackgroundJob;
use entities::ids::BackgroundJobId;
use entities::legacy_import::{
    LegacyImportBatch, LegacyImportBatchId, LegacyImportBatchStatus, LegacyImportRow, LegacyImportRowId,
};
use id_generator::next_id;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{CreateLegacyImportBatchRequest, LegacyImportBatchView};
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
        let background_job = BackgroundJob::for_legacy_import(
            BackgroundJobId::new(next_id()),
            &batch.batch_no,
            &batch.base.id,
            batch.total_rows,
            actor.id(),
        )?;
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

    /// 批量校验批次引用的资产存在（D05 仓储读取，INT-R27）。
    ///
    /// 单次 `$in` 取回全部可选资产事实并按输入顺序报告首个缺失标签；
    /// 空可选字段不访问数据库，重复 ID 只查询一次，软删除视为缺失。
    ///
    /// # 参数
    /// * `req` - 创建请求
    ///
    /// # 错误
    /// * `NotFound` - 资产引用不存在
    async fn ensure_file_assets_exist(&self, req: &CreateLegacyImportBatchRequest) -> Result<()> {
        let labeled: [(&str, Option<&entities::ids::FileAssetId>); 3] = [
            ("成功白名单包", req.successful_sanitized_file_asset_id.as_ref()),
            ("成功 manifest", req.success_manifest_file_asset_id.as_ref()),
            ("失败诊断包", req.failure_diagnostic_file_asset_id.as_ref()),
        ];
        let ids = labeled
            .iter()
            .filter_map(|(_, id)| (*id).cloned())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let missing = self
            .db
            .file_assets()
            .missing_file_asset_ids(&ids, &mut NoTransaction)
            .await?;
        if missing.is_empty() {
            return Ok(());
        }
        let label = first_missing_asset_label(&labeled, &missing).unwrap_or("资产".to_string());
        Err(Error::NotFound(format!("{label}资产不存在")))
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
    /// 经领域行工厂装配导入行实体（INT-E24）。
    ///
    /// Service 只预分配行 ID 并注入批次，字符串规范化与批内唯一判定由
    /// `entities::legacy_import::import_row_factory` 独占。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `batch_id` - 所属导入批次
    ///
    /// # 返回
    /// 返回新建的导入行实体列表。
    ///
    /// # 错误
    /// 行字段校验失败或批内来源身份重复时返回错误。
    fn build_rows(
        &self,
        req: &CreateLegacyImportBatchRequest,
        batch_id: &LegacyImportBatchId,
    ) -> Result<Vec<LegacyImportRow>> {
        let specs = req
            .rows
            .iter()
            .map(|row| entities::legacy_import::ImportRowSpec {
                row_id: LegacyImportRowId::new(next_id()),
                source_object_type: row.source_object_type.clone(),
                source_row_key: row.source_row_key.clone(),
                normalized_payload_reference: row.normalized_payload_reference.clone(),
            })
            .collect::<Vec<_>>();
        entities::legacy_import::build_import_rows(batch_id, specs).map_err(Into::into)
    }
}

/// 按输入顺序定位首个缺失资产的字段标签（INT-R27 纯映射）。
///
/// 保持 `ensure_file_assets_exist` 的字段标签化错误语义：重复 ID 只报告
/// 首次出现位置的标签，全部存在时返回 `None`。
///
/// # 参数
/// * `labeled` - 按请求字段顺序排列的标签与可选资产 ID
/// * `missing` - 仓储返回的缺失 ID（已去重，不保证顺序）
///
/// # 返回
/// 返回首个缺失资产的字段标签；无缺失时返回 `None`。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯内存映射，不访问数据库；不解释软删除与缺失的区分（由仓储决定）。
fn first_missing_asset_label(
    labeled: &[(&str, Option<&entities::ids::FileAssetId>); 3],
    missing: &[entities::ids::FileAssetId],
) -> Option<String> {
    use std::collections::HashSet;
    let missing_set = missing.iter().map(ToString::to_string).collect::<HashSet<_>>();
    for (label, id) in labeled {
        if let Some(id) = id {
            if missing_set.contains(id.as_ref()) {
                return Some((*label).to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::first_missing_asset_label;
    use entities::ids::FileAssetId;

    fn asset(id: &str) -> FileAssetId {
        FileAssetId::new(id.to_string())
    }

    #[test]
    fn empty_optional_fields_report_no_missing_label() {
        let labeled = [
            ("成功白名单包", None),
            ("成功 manifest", None),
            ("失败诊断包", None),
        ];
        let refs: [(&str, Option<&FileAssetId>); 3] = [
            (labeled[0].0, labeled[0].1.as_ref()),
            (labeled[1].0, labeled[1].1.as_ref()),
            (labeled[2].0, labeled[2].1.as_ref()),
        ];
        assert_eq!(first_missing_asset_label(&refs, &[]), None);
    }

    #[test]
    fn all_present_reports_no_missing_label() {
        let a = asset("a");
        let b = asset("b");
        let labeled = [
            ("成功白名单包", Some(&a)),
            ("成功 manifest", Some(&b)),
            ("失败诊断包", None),
        ];
        assert_eq!(first_missing_asset_label(&labeled, &[]), None);
    }

    #[test]
    fn partial_missing_reports_first_label_in_input_order() {
        let a = asset("a");
        let b = asset("b");
        let labeled = [
            ("成功白名单包", Some(&a)),
            ("成功 manifest", Some(&b)),
            ("失败诊断包", None),
        ];
        assert_eq!(
            first_missing_asset_label(&labeled, &[asset("b")]),
            Some("成功 manifest".to_string())
        );
        assert_eq!(
            first_missing_asset_label(&labeled, &[asset("a"), asset("b")]),
            Some("成功白名单包".to_string())
        );
    }

    #[test]
    fn duplicate_ids_report_first_occurrence_label() {
        let a = asset("a");
        let labeled = [
            ("成功白名单包", Some(&a)),
            ("成功 manifest", Some(&a)),
            ("失败诊断包", None),
        ];
        assert_eq!(
            first_missing_asset_label(&labeled, &[asset("a")]),
            Some("成功白名单包".to_string())
        );
    }
}
