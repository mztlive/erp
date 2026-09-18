use persistence_core::NoTransaction;
use validator::Validate;

use super::LegacyImportService;
use crate::dto::legacy_import::{
    LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchListQuery,
    LegacyImportBatchView, LegacyImportRowListParams, LegacyImportRowView, PageView,
};
use crate::entity::legacy_import::{LegacyImportBatch, LegacyImportBatchId};
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::repository::{
    LegacyImportBatchFilter, LegacyImportBatchRow, LegacyImportExt, LegacyImportRowFilter, LegacyImportRowRow,
};

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
        let page =
            self.db.legacy_import_batches().search_legacy_import_batches(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(LegacyImportBatchListItem::from).collect();

        Ok(page_view(items, page.total, filter.page, filter.page_size))
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
            sort_by: Some(query.paging.sort_field()),
            sort_ascending: query.paging.sort_ascending(),
        };
        let page =
            self.db.legacy_import_rows().search_legacy_import_rows(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(LegacyImportRowView::from).collect();

        Ok(page_view(items, page.total, filter.page, filter.page_size))
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
            sort_by: Some(query.paging.sort_field()),
            sort_ascending: query.paging.sort_ascending(),
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
        let background_job_id =
            self.bulk_jobs.background_job_id_by_request_id(&batch.batch_no, &mut NoTransaction).await?;
        let mut view: LegacyImportBatchView = batch.into();
        view.background_job_id = background_job_id;
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

/// 组装契约形状的分页视图（批次/行列表共用；分页口径不变）。
///
/// # 参数
/// * `items` - 当前页数据
/// * `total` - 满足筛选条件的总数
/// * `page` - 当前页码
/// * `page_size` - 单页条数
///
/// # 返回
/// 返回 `items`/`total`/`page`/`page_size` 视图。
fn page_view<Item>(items: Vec<Item>, total: i64, page: u64, page_size: u32) -> PageView<Item> {
    PageView { items, total, page, page_size }
}

impl From<LegacyImportBatchRow> for LegacyImportBatchListItem {
    /// 从批次列表投影行构造响应项（与详情 `From<LegacyImportBatch>` 风格统一）。
    ///
    /// # 参数
    /// * `row` - 批次列表投影行
    ///
    /// # 返回
    /// 返回列表响应项。
    fn from(row: LegacyImportBatchRow) -> Self {
        Self {
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
        }
    }
}

impl From<LegacyImportRowRow> for LegacyImportRowView {
    /// 从导入行列表投影行构造响应视图。
    ///
    /// # 参数
    /// * `row` - 导入行列表投影行
    ///
    /// # 返回
    /// 返回行响应视图。
    fn from(row: LegacyImportRowRow) -> Self {
        Self {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::page_view;

    #[test]
    fn page_view_carries_items_total_and_paging() {
        let view = page_view(vec!["row-1"], 7, 2, 20);
        assert_eq!(view.items, vec!["row-1"]);
        assert_eq!(view.total, 7);
        assert_eq!(view.page, 2);
        assert_eq!(view.page_size, 20);
    }
}
