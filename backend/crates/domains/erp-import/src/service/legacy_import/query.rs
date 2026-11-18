use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::legacy_import;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::LegacyImportService;
use crate::dto::legacy_import::{
    LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchView, LegacyImportRowListParams,
    LegacyImportRowView, PageView,
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
    /// * `actor` - 当前认证账号
    /// * `executor` - 调用方快照；关联展示补齐必须继续使用该执行器。
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// 非快照、分页或排序参数非法时返回 `ValidationError`；功能资格及仓储失败保留原错误。
    pub async fn batch_list(
        &self,
        params: &LegacyImportBatchListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<LegacyImportBatchListItem>> {
        ensure_batch_snapshot(executor)?;
        params.validate()?;
        self.authorization
            .require(actor, &legacy_import::BATCH_LIST, AccessTarget::Operation, executor)
            .await?;
        let query = params.normalized()?;
        let filter = LegacyImportBatchFilter {
            batch_no: query.batch_no,
            source_system_id: query.source_system_id,
            status: query.status,
            baseline_date_from: query.baseline_date_from,
            baseline_date_to: query.baseline_date_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_field()),
            sort_ascending: query.paging.sort_ascending(),
        };
        let page = self.db.legacy_import_batches().search_legacy_import_batches(&filter, executor).await?;
        let items = page.items.into_iter().map(LegacyImportBatchListItem::from).collect();

        Ok(page_view(items, page.total, filter.page, filter.page_size))
    }

    /// 查询导入批次详情（含后台任务关联）。
    ///
    /// # 参数
    /// * `id` - 导入批次 ID
    /// * `actor` - 当前认证账号
    /// * `executor` - 调用方快照；关联展示补齐必须继续使用该执行器。
    ///
    /// # 返回
    /// 返回批次的响应视图（含 `background_job_id`）。
    ///
    /// # 错误
    /// 非快照返回 `ValidationError`，批次不存在返回 `NotFound`；功能资格、批次及后台任务失败保留原错误。
    pub async fn batch_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<LegacyImportBatchView> {
        ensure_batch_snapshot(executor)?;
        self.authorization
            .require(actor, &legacy_import::BATCH_DETAIL, AccessTarget::Operation, executor)
            .await?;
        let batch = self
            .db
            .legacy_import_batches()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        self.batch_view_of(batch, executor).await
    }

    /// 分页查询导入行列表（按批次）。
    ///
    /// # 参数
    /// * `batch_id` - 所属导入批次
    /// * `params` - 查询参数
    /// * `actor` - 当前认证账号
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// 批次不存在时返回 `NotFound`。分页或排序参数非法时返回 `ValidationError`。
    /// 批次核对或行查询的仓储失败返回对应的导入领域错误，不限于 `RepositoryError`。
    pub async fn row_list(
        &self,
        batch_id: &str,
        params: &LegacyImportRowListParams,
        actor: &AuditActor,
    ) -> Result<PageView<LegacyImportRowView>> {
        let service = self.clone();
        let batch_id = batch_id.to_owned();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.row_list_snapshot(&batch_id, &params, &actor, executor).await })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn row_list_snapshot(
        &self,
        batch_id: &str,
        params: &LegacyImportRowListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<LegacyImportRowView>> {
        params.validate()?;
        self.authorization
            .require(actor, &legacy_import::ROW_LIST, AccessTarget::Operation, executor)
            .await?;
        self.ensure_batch_exists(batch_id, executor).await?;
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
        let page = self.db.legacy_import_rows().search_legacy_import_rows(&filter, executor).await?;
        let items = page.items.into_iter().map(LegacyImportRowView::from).collect();

        Ok(page_view(items, page.total, filter.page, filter.page_size))
    }

    /// 构造导入批次详情视图（补充 D04 后台任务关联）。
    ///
    /// # 参数
    /// * `batch` - 导入批次实体
    /// * `executor` - 公开查询已授权的原读取执行器
    ///
    /// # 返回
    /// 返回含 `background_job_id` 的响应视图。
    ///
    /// # 错误
    /// 后台任务身份查询失败时返回 [`crate::ports::BulkJobFactsPort`] 的错误，不是本方法直接查库。
    pub(super) async fn batch_view_of(
        &self,
        batch: LegacyImportBatch,
        executor: &mut dyn Executor,
    ) -> Result<LegacyImportBatchView> {
        let background_job_id =
            self.bulk_jobs.background_job_id_by_request_id(&batch.batch_no, executor).await?;
        let mut view: LegacyImportBatchView = batch.into();
        view.background_job_id = background_job_id;
        Ok(view)
    }

    /// 校验批次存在。
    ///
    /// # 参数
    /// * `batch_id` - 导入批次 ID
    ///
    /// # 返回
    /// 批次存在时无返回值。
    ///
    /// # 错误
    /// 批次不存在时返回 `NotFound`。仓储读取失败时返回对应的导入领域错误。
    async fn ensure_batch_exists(&self, batch_id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.db
            .legacy_import_batches()
            .find_by_id(batch_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        Ok(())
    }
}

/// 功能资格读取只允许在调用方持有的同一快照完成。
fn ensure_batch_snapshot(executor: &dyn Executor) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("导入批次读取必须使用快照事务".into()));
    }
    Ok(())
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
            source_system_name: None,
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

    /// 批次根与来源名称不得在非事务执行器上拆开读取。
    #[tokio::test]
    async fn batch_reads_reject_non_snapshot_before_identity_or_batch_reads() {
        use std::sync::Arc;

        use application_core::AuditActor;
        use erp_core::AccountKind;
        use erp_identity::testing::unavailable_authorization;
        use persistence_core::ReadExecutor;

        use super::{Error, LegacyImportService};
        use crate::dto::legacy_import::LegacyImportBatchListParams;
        use crate::ports::FailClosedBulkJobFacts;

        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service = LegacyImportService::new(
            client.database("legacy_import_batch_snapshot_test"),
            Arc::new(FailClosedBulkJobFacts),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let errors = [
            service
                .batch_list(&LegacyImportBatchListParams::default(), &actor, &mut ReadExecutor::default())
                .await
                .expect_err("非快照必须拒绝"),
            service
                .batch_detail("batch", &actor, &mut ReadExecutor::default())
                .await
                .expect_err("非快照必须拒绝"),
        ];
        for error in errors {
            assert!(
                matches!(error, Error::ValidationError(reason) if reason == "导入批次读取必须使用快照事务")
            );
        }
    }

    /// 管理读取的事务步骤均须先经身份设施，不能先探测真实批次或行。
    #[tokio::test]
    async fn import_reads_require_identity_before_querying_batches_or_rows() {
        use std::sync::Arc;

        use application_core::AuditActor;
        use erp_core::AccountKind;
        use erp_identity::testing::unavailable_authorization;
        use persistence_core::testing::SnapshotExecutor;

        use super::{Error, LegacyImportService};
        use crate::dto::legacy_import::{LegacyImportBatchListParams, LegacyImportRowListParams};
        use crate::ports::FailClosedBulkJobFacts;
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service = LegacyImportService::new(
            client.database("legacy_import_authorization_test"),
            Arc::new(FailClosedBulkJobFacts),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let rows: LegacyImportRowListParams = serde_json::from_value(serde_json::json!({})).unwrap();
        let errors = [
            service
                .batch_list(&LegacyImportBatchListParams::default(), &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("必须拒绝"),
            service
                .batch_detail("batch", &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("必须拒绝"),
            service
                .row_list_snapshot("batch", &rows, &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("必须拒绝"),
        ];
        for error in errors {
            assert!(matches!(error,Error::Internal(ref reason) if reason=="测试授权设施不可用"));
        }
    }

    #[test]
    fn page_view_carries_items_total_and_paging() {
        let view = page_view(vec!["row-1"], 7, 2, 20);
        assert_eq!(view.items, vec!["row-1"]);
        assert_eq!(view.total, 7);
        assert_eq!(view.page, 2);
        assert_eq!(view.page_size, 20);
    }
}
