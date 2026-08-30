use database::{BulkJobExt, LegacyImportExt, NoTransaction};
use entities::legacy_import::{LegacyImportBatch, LegacyImportBatchId};
use validator::Validate;

use crate::errors::{Error, Result};

use super::dto::{
    LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchListQuery,
    LegacyImportBatchView, LegacyImportRowListParams, LegacyImportRowView, PageView, SortDir,
};
use super::{LegacyImportBatchFilter, LegacyImportRowFilter, LegacyImportService};

impl LegacyImportService {
    /// 分页查询导入批次列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn batch_list(
        &self,
        params: &LegacyImportBatchListParams,
    ) -> Result<PageView<LegacyImportBatchListItem>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = self.batch_filter_of(&query);
        let page = self
            .db
            .legacy_import_batches()
            .search_legacy_import_batches(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| LegacyImportBatchListItem {
                id: row.id,
                batch_no: row.batch_no,
                source_system_id: row.source_system_id.to_string(),
                source_object_set: row.source_object_set,
                baseline_date: row.baseline_date,
                import_rule_version: row.import_rule_version,
                status: row.status,
                total_rows: row.total_rows,
                success_rows: row.success_rows,
                failed_rows: row.failed_rows,
                failure_code_summary: row.failure_code_summary,
                confirmation_status_summary: row.confirmation_status_summary,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询导入批次详情（含后台任务关联）。
    ///
    /// # 参数
    /// * `id` - 导入批次 ID
    ///
    /// # 返回
    /// 返回批次的响应视图（含 `background_job_id`）。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn batch_detail(&self, id: &str) -> Result<LegacyImportBatchView> {
        let batch = self
            .db
            .legacy_import_batches()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        self.batch_view_of(batch).await
    }

    /// 分页查询导入行列表（按批次）。
    ///
    /// # 参数
    /// * `batch_id` - 所属导入批次
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn row_list(
        &self,
        batch_id: &str,
        params: &LegacyImportRowListParams,
    ) -> Result<PageView<LegacyImportRowView>> {
        self.ensure_batch_exists(batch_id).await?;
        params.validate()?;
        let query = params.normalized()?;
        let filter = LegacyImportRowFilter {
            batch_id: Some(LegacyImportBatchId::new(batch_id.to_string())),
            parse_status: query.parse_status,
            mapping_status: query.mapping_status,
            import_status: query.import_status,
            source_row_key: query.source_row_key,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .legacy_import_rows()
            .search_legacy_import_rows(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| LegacyImportRowView {
                id: row.id,
                batch_id: row.batch_id.to_string(),
                source_object_type: row.source_object_type,
                source_row_key: row.source_row_key,
                parse_status: row.parse_status,
                mapping_status: row.mapping_status,
                import_status: row.import_status,
                external_identity_map_id: row.external_identity_map_id.map(|id| id.to_string()),
                error_code: row.error_code,
                target_document_id: row.target_document_id,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 构造导入批次列表筛选条件。
    ///
    /// # 参数
    /// * `query` - 归一化查询参数
    ///
    /// # 返回
    /// 返回仓储筛选条件。
    fn batch_filter_of(&self, query: &LegacyImportBatchListQuery) -> LegacyImportBatchFilter {
        LegacyImportBatchFilter {
            batch_no: query.batch_no.clone(),
            source_system_id: query.source_system_id.clone(),
            status: query.status,
            baseline_date_from: query.baseline_date_from,
            baseline_date_to: query.baseline_date_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        }
    }

    /// 构造导入批次详情视图（补充 D04 后台任务关联）。
    ///
    /// # 参数
    /// * `batch` - 导入批次实体
    ///
    /// # 返回
    /// 返回含 `background_job_id` 的响应视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    pub(super) async fn batch_view_of(&self, batch: LegacyImportBatch) -> Result<LegacyImportBatchView> {
        let background_job = self
            .db
            .background_jobs()
            .find_by_request_id(&batch.batch_no, &mut NoTransaction)
            .await?;
        let mut view: LegacyImportBatchView = batch.into();
        view.background_job_id = background_job.map(|job| job.base.id);
        Ok(view)
    }

    /// 校验批次存在。
    ///
    /// # 参数
    /// * `batch_id` - 导入批次 ID
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    async fn ensure_batch_exists(&self, batch_id: &str) -> Result<()> {
        self.db
            .legacy_import_batches()
            .find_by_id(batch_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        Ok(())
    }
}
