use std::future::Future;
use std::pin::Pin;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::legacy_import;
use persistence_core::{Executor, PageResult, Transactional};
use validator::Validate;

use super::LegacyImportService;
use crate::dto::legacy_import::{
    LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchListQuery,
    LegacyImportBatchView, LegacyImportRowListParams, LegacyImportRowView, PageView,
};
use crate::entity::legacy_import::{LegacyImportBatch, LegacyImportBatchId};
use crate::error::{Error, Result};
use crate::ports::BulkJobFactsPort;
use crate::repository::prelude::*;
use crate::repository::{
    LegacyImportBatchFilter, LegacyImportBatchRow, LegacyImportExt, LegacyImportRowFilter, LegacyImportRowRow,
};

type ReadResult<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

impl LegacyImportService {
    /// 分页查询导入批次列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 当前认证账号
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// 分页或排序参数非法时返回 `ValidationError`。仓储查询失败时返回对应的导入领域错误，不限于 `RepositoryError`。
    pub async fn batch_list(
        &self,
        params: &LegacyImportBatchListParams,
        actor: &AuditActor,
    ) -> Result<PageView<LegacyImportBatchListItem>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let db = service.db;
                    batch_list(&service.authorization, &params, &actor, executor, move |filter, executor| {
                        Box::pin(async move {
                            Ok(db
                                .legacy_import_batches()
                                .search_legacy_import_batches(filter, executor)
                                .await?)
                        })
                    })
                    .await
                })
            })
            .await
    }

    /// 查询导入批次详情（含后台任务关联）。
    ///
    /// # 参数
    /// * `id` - 导入批次 ID
    /// * `actor` - 当前认证账号
    ///
    /// # 返回
    /// 返回批次的响应视图（含 `background_job_id`）。
    ///
    /// # 错误
    /// 批次不存在时返回 `NotFound`。批次读取失败时返回对应的导入领域错误。后台任务身份查询失败时返回端口错误。
    pub async fn batch_detail(&self, id: &str, actor: &AuditActor) -> Result<LegacyImportBatchView> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let db = service.db;
                    batch_detail(
                        &service.authorization,
                        service.bulk_jobs.as_ref(),
                        &id,
                        &actor,
                        executor,
                        move |id, executor| {
                            Box::pin(
                                async move { Ok(db.legacy_import_batches().find_by_id(id, executor).await?) },
                            )
                        },
                    )
                    .await
                })
            })
            .await
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
                Box::pin(async move {
                    let db = service.db;
                    let batches = db.clone();
                    row_list(
                        &service.authorization,
                        &batch_id,
                        &params,
                        &actor,
                        executor,
                        move |id, executor| {
                            Box::pin(async move {
                                Ok(batches.legacy_import_batches().find_by_id(id, executor).await?)
                            })
                        },
                        move |filter, executor| {
                            Box::pin(async move {
                                Ok(db
                                    .legacy_import_rows()
                                    .search_legacy_import_rows(filter, executor)
                                    .await?)
                            })
                        },
                    )
                    .await
                })
            })
            .await
    }
}

// 公开入口管理事务；私有步骤允许替换 I/O，授权、筛选和结果构造共用生产实现。
async fn batch_list(
    authorization: &AuthorizationService,
    params: &LegacyImportBatchListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
    read: impl for<'a> FnOnce(
        &'a LegacyImportBatchFilter,
        &'a mut dyn Executor,
    ) -> ReadResult<'a, PageResult<LegacyImportBatchRow>>,
) -> Result<PageView<LegacyImportBatchListItem>> {
    params.validate()?;
    authorization.require(actor, &legacy_import::BATCH_LIST, AccessTarget::Operation, executor).await?;
    let filter = batch_filter_of(params.normalized()?);
    let page = read(&filter, executor).await?;
    let items = page.items.into_iter().map(LegacyImportBatchListItem::from).collect();
    Ok(page_view(items, page.total, filter.page, filter.page_size))
}

async fn batch_detail(
    authorization: &AuthorizationService,
    bulk_jobs: &dyn BulkJobFactsPort,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
    read: impl for<'a> FnOnce(&'a str, &'a mut dyn Executor) -> ReadResult<'a, Option<LegacyImportBatch>>,
) -> Result<LegacyImportBatchView> {
    authorization.require(actor, &legacy_import::BATCH_DETAIL, AccessTarget::Operation, executor).await?;
    let batch = read(id, executor).await?.ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
    let background_job_id = bulk_jobs.background_job_id_by_request_id(&batch.batch_no, executor).await?;
    let mut view: LegacyImportBatchView = batch.into();
    view.background_job_id = background_job_id;
    Ok(view)
}

async fn row_list(
    authorization: &AuthorizationService,
    batch_id: &str,
    params: &LegacyImportRowListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
    read_batch: impl for<'a> FnOnce(&'a str, &'a mut dyn Executor) -> ReadResult<'a, Option<LegacyImportBatch>>,
    read_rows: impl for<'a> FnOnce(
        &'a LegacyImportRowFilter,
        &'a mut dyn Executor,
    ) -> ReadResult<'a, PageResult<LegacyImportRowRow>>,
) -> Result<PageView<LegacyImportRowView>> {
    params.validate()?;
    authorization.require(actor, &legacy_import::ROW_LIST, AccessTarget::Operation, executor).await?;
    read_batch(batch_id, executor).await?.ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
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
    let page = read_rows(&filter, executor).await?;
    let items = page.items.into_iter().map(LegacyImportRowView::from).collect();
    Ok(page_view(items, page.total, filter.page, filter.page_size))
}

/// 将归一化查询参数交给仓储，保留原筛选、分页及排序口径。
fn batch_filter_of(query: LegacyImportBatchListQuery) -> LegacyImportBatchFilter {
    LegacyImportBatchFilter {
        batch_no: query.batch_no,
        source_system_id: query.source_system_id,
        status: query.status,
        baseline_date_from: query.baseline_date_from,
        baseline_date_to: query.baseline_date_to,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_field()),
        sort_ascending: query.paging.sort_ascending(),
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
    use std::slice;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use application_core::ErrorClass;
    use async_trait::async_trait;
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::SourceSystemId;
    use erp_identity::testing::{AuthorizationFixture, unavailable_authorization};
    use erp_permissions::Permission;
    use mongodb::error::Error as MongoError;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{Error as RepositoryError, ExecutionBoundary, NoTransaction, ReadExecutor};

    use super::*;
    use crate::entity::legacy_import::{ImportStatus, LegacyImportBatchStatus, MappingStatus, ParseStatus};
    use crate::ports::FailClosedBulkJobFacts;

    const READS: [Permission; 3] =
        [legacy_import::BATCH_LIST, legacy_import::BATCH_DETAIL, legacy_import::ROW_LIST];

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    fn authorization(permissions: &[Permission]) -> Arc<AuthorizationService> {
        AuthorizationFixture::new(&[("reader", permissions)]).actor(actor()).build()
    }

    fn rows_params() -> LegacyImportRowListParams {
        serde_json::from_value(serde_json::json!({})).unwrap()
    }

    fn batch() -> LegacyImportBatch {
        let mut base = BaseModel::fake();
        base.id = "batch-1".into();
        LegacyImportBatch {
            base,
            batch_no: "IMP-1".into(),
            source_system_id: SourceSystemId::new("source-1"),
            source_object_set: "CUSTOMER".into(),
            baseline_date: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            successful_sanitized_file_asset_id: None,
            success_manifest_file_asset_id: None,
            failure_diagnostic_file_asset_id: None,
            import_rule_version: "v1".into(),
            source_file_hmac: None,
            status: LegacyImportBatchStatus::PendingValidation,
            total_rows: 7,
            success_rows: 0,
            failed_rows: 0,
            failure_code_summary: None,
            confirmation_status_summary: None,
        }
    }

    fn batch_row() -> LegacyImportBatchRow {
        serde_json::from_value(serde_json::to_value(batch()).unwrap()).unwrap()
    }

    fn row() -> LegacyImportRowRow {
        LegacyImportRowRow {
            id: "row-1".into(),
            batch_id: LegacyImportBatchId::new("batch-1"),
            source_object_type: "CUSTOMER".into(),
            source_row_key: "customer-1".into(),
            parse_status: ParseStatus::Valid,
            mapping_status: MappingStatus::Mapped,
            import_status: ImportStatus::Imported,
            external_identity_map_id: None,
            error_code: None,
            target_document_id: Some("customer-erp-1".into()),
            version: 2,
            created_at: 10,
        }
    }

    // 回调一旦被调用就失败，正常缺权不能被设施故障或空数据掩盖。
    async fn refused_read(
        index: usize,
        service: &AuthorizationService,
        executor: &mut dyn Executor,
    ) -> Error {
        match index {
            0 => batch_list(service, &LegacyImportBatchListParams::default(), &actor(), executor, |_, _| {
                panic!("未授权时禁止读取批次页或计数")
            })
            .await
            .unwrap_err(),
            1 => batch_detail(service, &FailClosedBulkJobFacts, "batch-1", &actor(), executor, |_, _| {
                panic!("未授权时禁止探测批次存在性")
            })
            .await
            .unwrap_err(),
            2 => row_list(
                service,
                "batch-1",
                &rows_params(),
                &actor(),
                executor,
                |_, _| panic!("未授权时禁止探测批次存在性"),
                |_, _| panic!("未授权时禁止读取行或计数"),
            )
            .await
            .unwrap_err(),
            _ => panic!("未知测试入口"),
        }
    }

    #[tokio::test]
    async fn missing_or_other_read_permission_never_reaches_business_reads() {
        for (index, required) in READS.iter().enumerate() {
            assert!(matches!(
                refused_read(index, &authorization(&[]), &mut SnapshotExecutor::default()).await,
                Error::Forbidden(_)
            ));
            for granted in READS.iter().filter(|permission| *permission != required) {
                let service = authorization(slice::from_ref(granted));
                assert!(matches!(
                    refused_read(index, &service, &mut SnapshotExecutor::default()).await,
                    Error::Forbidden(_)
                ));
            }
        }
    }

    #[tokio::test]
    async fn identity_failure_never_reaches_business_reads() {
        for index in 0..READS.len() {
            let error =
                refused_read(index, &unavailable_authorization(), &mut SnapshotExecutor::default()).await;
            assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
        }
    }

    #[tokio::test]
    async fn unresolved_actor_never_reaches_business_reads() {
        let service = AuthorizationFixture::new(&[("reader", &READS)])
            .actor(AuditActor::new("another-user".into(), "another-login".into(), AccountKind::Admin))
            .build();
        for index in 0..READS.len() {
            assert!(matches!(
                refused_read(index, &service, &mut SnapshotExecutor::default()).await,
                Error::Forbidden(_)
            ));
        }
    }

    #[tokio::test]
    async fn non_snapshot_and_revoked_executors_are_rejected_before_reads() {
        let service = authorization(&READS);
        for index in 0..READS.len() {
            let mut revoked = SnapshotExecutor::default();
            revoked.session();
            let errors = [
                refused_read(index, &service, &mut NoTransaction).await,
                refused_read(index, &service, &mut ReadExecutor::default()).await,
                refused_read(index, &service, &mut revoked).await,
            ];
            for error in errors {
                assert!(
                    matches!(error, Error::ValidationError(reason) if reason == "无对象操作及其业务读取必须使用快照事务")
                );
            }
        }
    }

    #[tokio::test]
    async fn batch_list_delivers_page_and_filter_using_the_original_executor() {
        let mut executor = SnapshotExecutor::default();
        let observed = Arc::new(Mutex::new(None));
        let boundary = observed.clone();
        let params = LegacyImportBatchListParams {
            batch_no: Some(" IMP ".into()),
            source_system_id: Some(SourceSystemId::new("source-1")),
            page: Some(2),
            page_size: Some(3),
            sort_by: Some("batch_no".into()),
            sort_dir: Some("asc".into()),
            ..Default::default()
        };
        let view = batch_list(
            &authorization(&[legacy_import::BATCH_LIST]),
            &params,
            &actor(),
            &mut executor,
            move |filter, executor| {
                Box::pin(async move {
                    assert_eq!(filter.batch_no.as_deref(), Some("IMP"));
                    assert_eq!(filter.source_system_id, Some(SourceSystemId::new("source-1")));
                    assert_eq!((filter.page, filter.page_size), (2, 3));
                    assert_eq!(filter.sort_by.as_deref(), Some("batch_no"));
                    assert!(filter.sort_ascending);
                    *boundary.lock().unwrap() = executor.boundary();
                    Ok(PageResult { items: vec![batch_row()], total: 7 })
                })
            },
        )
        .await
        .unwrap();
        assert_eq!((view.total, view.page, view.page_size), (7, 2, 3));
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].id, "batch-1");
        assert_eq!(view.items[0].batch_no, "IMP-1");
        assert_eq!(view.items[0].total_rows, 7);
        assert!(observed.lock().unwrap().as_ref().unwrap().is_current(&executor));
    }

    struct Jobs {
        response: Mutex<Option<Result<Option<String>>>>,
        boundary: Mutex<Option<ExecutionBoundary>>,
        calls: AtomicUsize,
    }

    impl Jobs {
        fn new(response: Result<Option<String>>) -> Self {
            Self {
                response: Mutex::new(Some(response)),
                boundary: Mutex::new(None),
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl BulkJobFactsPort for Jobs {
        async fn background_job_id_by_request_id(
            &self,
            request_id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<String>> {
            assert_eq!(request_id, "IMP-1");
            assert!(self.boundary.lock().unwrap().as_ref().unwrap().is_current(executor));
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.response.lock().unwrap().take().expect("端口只读取一次")
        }
    }

    #[tokio::test]
    async fn detail_delivers_job_identity_or_none_using_the_batch_executor() {
        for job_id in [Some("job-1".to_string()), None] {
            let jobs = Arc::new(Jobs::new(Ok(job_id.clone())));
            let observed = jobs.clone();
            let mut executor = SnapshotExecutor::default();
            let view = batch_detail(
                &authorization(&[legacy_import::BATCH_DETAIL]),
                jobs.as_ref(),
                "batch-1",
                &actor(),
                &mut executor,
                move |id, executor| {
                    Box::pin(async move {
                        assert_eq!(id, "batch-1");
                        *observed.boundary.lock().unwrap() = executor.boundary();
                        Ok(Some(batch()))
                    })
                },
            )
            .await
            .unwrap();
            let mut expected = LegacyImportBatchView::from(batch());
            expected.background_job_id = job_id;
            assert_eq!(view, expected);
            assert_eq!(jobs.calls.load(Ordering::SeqCst), 1);
            assert!(jobs.boundary.lock().unwrap().as_ref().unwrap().is_current(&executor));
        }
    }

    #[tokio::test]
    async fn detail_missing_batch_and_read_failure_stop_before_job_lookup() {
        let service = authorization(&[legacy_import::BATCH_DETAIL]);
        let jobs = Jobs::new(Ok(Some("unused".into())));
        let missing =
            batch_detail(&service, &jobs, "missing", &actor(), &mut SnapshotExecutor::default(), |_, _| {
                Box::pin(async { Ok(None) })
            })
            .await
            .unwrap_err();
        assert!(matches!(missing, Error::NotFound(reason) if reason == "导入批次不存在"));
        let failed =
            batch_detail(&service, &jobs, "batch-1", &actor(), &mut SnapshotExecutor::default(), |_, _| {
                Box::pin(async { Err(Error::Internal("batch storage unavailable".into())) })
            })
            .await
            .unwrap_err();
        assert!(matches!(failed, Error::Internal(reason) if reason == "batch storage unavailable"));
        assert_eq!(jobs.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn job_port_failure_is_propagated_instead_of_returning_an_incomplete_detail() {
        let jobs = Arc::new(Jobs::new(Err(Error::Internal("job storage unavailable".into()))));
        let observed = jobs.clone();
        let error = batch_detail(
            &authorization(&[legacy_import::BATCH_DETAIL]),
            jobs.as_ref(),
            "batch-1",
            &actor(),
            &mut SnapshotExecutor::default(),
            move |_, executor| {
                Box::pin(async move {
                    *observed.boundary.lock().unwrap() = executor.boundary();
                    Ok(Some(batch()))
                })
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "job storage unavailable"));
        assert_eq!(jobs.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rows_deliver_page_with_batch_filter_and_the_original_executor() {
        let mut executor = SnapshotExecutor::default();
        let observed = Arc::new(Mutex::new(None));
        let batch_boundary = observed.clone();
        let rows_boundary = observed.clone();
        let params = LegacyImportRowListParams {
            page: Some(2),
            page_size: Some(3),
            source_row_key: Some(" customer ".into()),
            ..rows_params()
        };
        let view = row_list(
            &authorization(&[legacy_import::ROW_LIST]),
            "batch-1",
            &params,
            &actor(),
            &mut executor,
            move |id, executor| {
                Box::pin(async move {
                    assert_eq!(id, "batch-1");
                    *batch_boundary.lock().unwrap() = executor.boundary();
                    Ok(Some(batch()))
                })
            },
            move |filter, executor| {
                Box::pin(async move {
                    assert!(rows_boundary.lock().unwrap().as_ref().unwrap().is_current(executor));
                    assert_eq!(filter.batch_id, Some(LegacyImportBatchId::new("batch-1")));
                    assert_eq!(filter.source_row_key.as_deref(), Some("customer"));
                    assert_eq!((filter.page, filter.page_size), (2, 3));
                    Ok(PageResult { items: vec![row()], total: 7 })
                })
            },
        )
        .await
        .unwrap();
        assert_eq!((view.total, view.page, view.page_size), (7, 2, 3));
        assert_eq!(view.items, vec![LegacyImportRowView::from(row())]);
        assert!(observed.lock().unwrap().as_ref().unwrap().is_current(&executor));
    }

    #[tokio::test]
    async fn row_reads_stop_when_batch_is_missing_or_batch_lookup_fails() {
        let service = authorization(&[legacy_import::ROW_LIST]);
        for (result, expected_class) in [
            (Ok(None), ErrorClass::BusinessRule),
            (Err(Error::Internal("batch storage unavailable".into())), ErrorClass::Internal),
        ] {
            let error = row_list(
                &service,
                "batch-1",
                &rows_params(),
                &actor(),
                &mut SnapshotExecutor::default(),
                move |_, _| Box::pin(async move { result }),
                |_, _| panic!("批次读取失败后禁止读取行或计数"),
            )
            .await
            .unwrap_err();
            assert_eq!(error.class(), expected_class);
            assert!(
                matches!(error, Error::NotFound(ref reason) if reason == "导入批次不存在")
                    || matches!(error, Error::Internal(ref reason) if reason == "batch storage unavailable")
            );
        }
    }

    #[tokio::test]
    async fn list_read_failures_are_propagated() {
        let service = authorization(&READS);
        let batches = batch_list(
            &service,
            &LegacyImportBatchListParams::default(),
            &actor(),
            &mut SnapshotExecutor::default(),
            |_, _| {
                Box::pin(async {
                    Err(Error::from(RepositoryError::DatabaseError(MongoError::custom(
                        "batch page unavailable",
                    ))))
                })
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(batches, Error::RepositoryError(RepositoryError::DatabaseError(error)) if error.get_custom::<&str>().copied() == Some("batch page unavailable"))
        );
        let rows = row_list(
            &service,
            "batch-1",
            &rows_params(),
            &actor(),
            &mut SnapshotExecutor::default(),
            |_, _| Box::pin(async { Ok(Some(batch())) }),
            |_, _| Box::pin(async { Err(Error::Internal("row page unavailable".into())) }),
        )
        .await
        .unwrap_err();
        assert!(matches!(rows, Error::Internal(reason) if reason == "row page unavailable"));
    }

    #[tokio::test]
    async fn out_of_range_pages_keep_totals_and_requested_paging() {
        let service = authorization(&READS);
        let params = LegacyImportBatchListParams { page: Some(4), page_size: Some(3), ..Default::default() };
        let view = batch_list(&service, &params, &actor(), &mut SnapshotExecutor::default(), |_, _| {
            Box::pin(async { Ok(PageResult { items: vec![], total: 7 }) })
        })
        .await
        .unwrap();
        assert!(view.items.is_empty());
        assert_eq!((view.total, view.page, view.page_size), (7, 4, 3));
        let params = LegacyImportRowListParams { page: Some(4), page_size: Some(3), ..rows_params() };
        let view = row_list(
            &service,
            "batch-1",
            &params,
            &actor(),
            &mut SnapshotExecutor::default(),
            |_, _| Box::pin(async { Ok(Some(batch())) }),
            |_, _| Box::pin(async { Ok(PageResult { items: vec![], total: 7 }) }),
        )
        .await
        .unwrap();
        assert!(view.items.is_empty());
        assert_eq!((view.total, view.page, view.page_size), (7, 4, 3));
    }

    #[tokio::test]
    async fn invalid_paging_stops_before_identity_and_reads() {
        let service = unavailable_authorization();
        let params = LegacyImportBatchListParams { page: Some(0), ..Default::default() };
        let error = batch_list(&service, &params, &actor(), &mut SnapshotExecutor::default(), |_, _| {
            panic!("非法分页不能读取批次")
        })
        .await
        .unwrap_err();
        assert_eq!(error.class(), ErrorClass::BusinessRule);
        let params = LegacyImportRowListParams { page_size: Some(0), ..rows_params() };
        let error = row_list(
            &service,
            "batch-1",
            &params,
            &actor(),
            &mut SnapshotExecutor::default(),
            |_, _| panic!("非法分页不能探测批次"),
            |_, _| panic!("非法分页不能读取行"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.class(), ErrorClass::BusinessRule);
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
