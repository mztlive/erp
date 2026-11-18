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
    /// 在调用方快照中检查认证账号的列表资格，再按白名单排序读批次投影和总数，返回 `items`、`total`、`page`、`page_size`。非快照或分页、排序非法时返回 `ValidationError`；授权及仓储失败保留原分类。
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

    /// 在原快照内检查详情资格、读取批次并补齐后台任务 ID。非快照返回 `ValidationError`，批次缺失返回 `NotFound`，授权、批次和任务读取失败沿用原错误。
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

    /// 为指定批次的导入行列表建立读取快照，使授权、批次存在校验、分页及计数一致。批次不存在返回 `NotFound`，分页或排序无效返回 `ValidationError`；其它授权和仓储失败保留对应领域错误。
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

    // 在公开入口建立的同一快照中检查列表资格、确认批次存在，再按状态及来源行键查询并转换分页结果。
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

    // 用已授权的原执行器，按批次号通过 `BulkJobFactsPort` 查后台任务 ID 并补入详情；端口失败直接返回，服务不绕过端口查任务库。
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

    // 在当前快照验证父批次真实存在，避免把不存在的批次误报为空列表；没有记录返回 `NotFound`，读取失败保留领域错误。
    async fn ensure_batch_exists(&self, batch_id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.db
            .legacy_import_batches()
            .find_by_id(batch_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        Ok(())
    }
}

// 在授权或批次读取前拒绝非快照执行器，保证资格、批次与关联读取不被拆成多个边界。
fn ensure_batch_snapshot(executor: &dyn Executor) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("导入批次读取必须使用快照事务".into()));
    }
    Ok(())
}

// 统一批次、行列表的响应分页结构，用当前页结果和仓储总数保留原分页口径。
fn page_view<Item>(items: Vec<Item>, total: i64, page: u64, page_size: u32) -> PageView<Item> {
    PageView { items, total, page, page_size }
}

impl From<LegacyImportBatchRow> for LegacyImportBatchListItem {
    // 将批次列表投影映射成展示项，不走详情取数或后台任务补齐路径。
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
    // 将仓储行投影转成列表响应，只展示投影允许的身份、状态及诊断信息。
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

    // 验证非快照读取在接触身份设施或批次存储前失败，避免分开读取批次和关联事实。
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

    // 验证批次或行的管理查询先经身份设施，不能在授权失败时探测业务数据。
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
