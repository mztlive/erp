//! 域 D04 `bulk_job` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建选择快照 + 冻结目标、创建后台任务 + 逐项结果：跨集合多步骤写入 →
//!   `with_transaction`（复用域专用仓储 `create_snapshot_with_items` /
//!   `create_job_with_items`）+ 审计日志同事务；
//! - 确认/失效/取消：业务行 + 审计日志 → `with_transaction`；
//! - 查询一律 `&mut NoTransaction`。
//!
//! 幂等：后台任务按 `request_id` 唯一索引幂等，重复提交返回既有任务
//! （§6.1：涉及资金/状态机变更的入口必须有幂等键）。
//!
//! 跨域：只经 `DatabaseExt` 调对方域 Repository（P3-service-api §2）。本域依赖
//! D02：批量冻结目标命中 `DocumentType` 目录（判定来自 entities）时，目标单据
//! 必须已在 D02 注册。

use database::{AccessControlExt, BulkJobExt, DocumentRegistryExt, NoTransaction, Transactional};
use entities::bulk_job::{
    BackgroundJob, BackgroundJobData, BackgroundJobId, BackgroundJobItem, BackgroundJobItemData,
    BulkSelectionItem, BulkSelectionItemData, BulkSelectionSnapshot, BulkSelectionSnapshotData,
    BulkSelectionSnapshotId,
};
use entities::document_registry::DocumentType;
use entities::ids::FileAssetId;
use id_generator::next_id;
use mongodb::Database;
use serde::Deserialize;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    BackgroundJobItemView, BackgroundJobListParams, BackgroundJobView, BulkSelectionItemView,
    BulkSelectionSnapshotListParams, BulkSelectionSnapshotView, CancelBackgroundJobRequest,
    ConfirmBulkSelectionSnapshotRequest, CreateBackgroundJobItemRequest, CreateBackgroundJobRequest,
    CreateBulkSelectionItemRequest, CreateBulkSelectionSnapshotRequest, ExpireBulkSelectionSnapshotRequest,
    PageView,
};

/// 选择快照列表筛选条件类型（经 `BulkJobExt` 关联类型跨 crate 可达）。
type BulkSelectionSnapshotFilter = <mongodb::Database as BulkJobExt>::BulkSelectionSnapshotFilter;
/// 后台任务列表筛选条件类型。
type BackgroundJobFilter = <mongodb::Database as BulkJobExt>::BackgroundJobFilter;

/// 批量任务服务。
///
/// 提供选择快照与后台任务的创建、确认与查询编排。
pub struct BulkJobService {
    db: Database,
}

impl BulkJobService {
    /// 创建批量任务服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询选择快照列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`selection_type`/`status`/`created_by` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn bulk_selection_snapshot_list(
        &self,
        params: &BulkSelectionSnapshotListParams,
    ) -> Result<PageView<BulkSelectionSnapshotView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = BulkSelectionSnapshotFilter {
            selection_type: query.selection_type,
            status: query.status,
            created_by: query.created_by,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .bulk_selection_snapshots()
            .search_snapshots(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| BulkSelectionSnapshotView {
                id: row.id,
                selection_type: row.selection_type,
                data_cutoff_at: row.data_cutoff_at,
                item_count: row.item_count,
                created_by: row.created_by,
                expires_at: row.expires_at,
                status: row.status,
                version: 0,
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

    /// 创建选择快照并冻结逐项目标（跨集合事务写入）。
    ///
    /// 「快照 + 冻结目标集合」原子可见（§6.1）；冻结目标命中 `DocumentType`
    /// 目录时校验目标单据已在 D02 注册。唯一性由
    /// `(selection_snapshot_id, object_type, object_id)` 唯一索引承担。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的快照视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 业务单据类目标未注册
    /// * `ConflictError` - 唯一索引冲突（透出）
    pub async fn create_bulk_selection_snapshot(
        &self,
        req: CreateBulkSelectionSnapshotRequest,
        actor: &AuditActor,
    ) -> Result<BulkSelectionSnapshotView> {
        req.validate()?;
        self.ensure_items_registered(&req.items).await?;
        let snapshot_id = BulkSelectionSnapshotId::new(next_id());
        let snapshot = BulkSelectionSnapshot::new(
            snapshot_id.clone(),
            BulkSelectionSnapshotData {
                selection_type: req.selection_type,
                data_cutoff_at: entities::common::time::Instant::from_unix_secs(req.data_cutoff_at as i64),
                item_count: req.items.len() as u32,
                created_by: actor.id().to_string(),
                expires_at: entities::common::time::Instant::from_unix_secs(req.expires_at as i64),
            },
        )?;
        let items = build_selection_items(&snapshot_id, req.items)?;
        let audit = actor.clone().resource_log(
            "bulk_selection_snapshot.create",
            "bulk_selection_snapshot",
            snapshot.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let snapshot_for_tx = snapshot.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.bulk_job()
                        .create_snapshot_with_items(&snapshot_for_tx, items, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(snapshot.into())
    }

    /// 确认选择快照。
    ///
    /// 快照确认后目标集合、截止水位和预期版本不可修改（§6.1，实体无目标字段
    /// 变更方法）。
    ///
    /// # 参数
    /// * `id` - 快照 ID
    /// * `req` - 确认请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的快照视图。
    ///
    /// # 错误
    /// * `NotFound` - 快照不存在
    /// * `ConflictError` - 版本陈旧或状态机不允许确认
    pub async fn confirm_bulk_selection_snapshot(
        &self,
        id: &str,
        req: ConfirmBulkSelectionSnapshotRequest,
        actor: &AuditActor,
    ) -> Result<BulkSelectionSnapshotView> {
        req.validate()?;
        let mut snapshot = self.load_snapshot_with_version(id, req.version).await?;
        snapshot.confirm()?;
        self.update_snapshot_with_audit(snapshot, "bulk_selection_snapshot.confirm", actor)
            .await
    }

    /// 标记选择快照失效。
    ///
    /// 仅 `PENDING` / `CONFIRMED` 可失效；执行中的快照只能走向完成（实体状态机）。
    ///
    /// # 参数
    /// * `id` - 快照 ID
    /// * `req` - 失效请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回失效后的快照视图。
    ///
    /// # 错误
    /// * `NotFound` - 快照不存在
    /// * `ConflictError` - 版本陈旧或状态机不允许失效
    pub async fn expire_bulk_selection_snapshot(
        &self,
        id: &str,
        req: ExpireBulkSelectionSnapshotRequest,
        actor: &AuditActor,
    ) -> Result<BulkSelectionSnapshotView> {
        req.validate()?;
        let mut snapshot = self.load_snapshot_with_version(id, req.version).await?;
        snapshot.expire()?;
        self.update_snapshot_with_audit(snapshot, "bulk_selection_snapshot.expire", actor)
            .await
    }

    /// 分页查询快照逐项结果。
    ///
    /// # 参数
    /// * `snapshot_id` - 选择快照 ID
    /// * `result_status` - 逐项执行结果筛选
    /// * `page` - 页码（1 起）
    /// * `page_size` - 单页条数
    ///
    /// # 返回
    /// 返回当前页逐项结果行与总数。
    ///
    /// # 错误
    /// 分页参数非法或数据库查询失败时返回错误。
    pub async fn bulk_selection_item_list(
        &self,
        snapshot_id: &str,
        result_status: Option<entities::bulk_job::SelectionItemStatus>,
        page: u64,
        page_size: u32,
    ) -> Result<PageView<BulkSelectionItemView>> {
        if page == 0 {
            return Err(Error::ValidationError("页码必须大于0".to_string()));
        }
        let snapshot_id = BulkSelectionSnapshotId::new(snapshot_id);
        let result = self
            .db
            .bulk_selection_items()
            .search_items(&snapshot_id, result_status, page, page_size, &mut NoTransaction)
            .await?;
        let items = result
            .items
            .into_iter()
            .map(|row| BulkSelectionItemView {
                id: row.id,
                selection_snapshot_id: snapshot_id.to_string(),
                object_type: row.object_type,
                object_id: row.object_id,
                expected_version: row.expected_version,
                expected_hash: row.expected_hash,
                result_status: row.result_status,
                result_code: row.result_code,
            })
            .collect();

        Ok(PageView {
            items,
            total: result.total,
            page,
            page_size,
        })
    }

    /// 分页查询后台任务列表（任务中心）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`job_no`/`job_type`/`status`/`requested_by` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn background_job_list(
        &self,
        params: &BackgroundJobListParams,
    ) -> Result<PageView<BackgroundJobView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = BackgroundJobFilter {
            job_no: query.job_no,
            job_type: query.job_type,
            status: query.status,
            requested_by: query.requested_by,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .background_jobs()
            .search_background_jobs(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| BackgroundJobView {
                id: row.id,
                job_no: row.job_no,
                job_type: row.job_type,
                domain_job_type: None,
                domain_job_id: None,
                selection_snapshot_id: None,
                requested_by: row.requested_by,
                request_id: row.request_id,
                input_file_asset_id: None,
                result_file_asset_id: None,
                status: row.status,
                total_count: row.total_count,
                processed_count: row.processed_count,
                success_count: row.success_count,
                skipped_count: row.skipped_count,
                failed_count: row.failed_count,
                started_at: None,
                finished_at: None,
                last_progress_at: None,
                result_expires_at: row.result_expires_at,
                error_summary: None,
                version: 0,
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

    /// 查询后台任务详情。
    ///
    /// # 参数
    /// * `id` - 后台任务 ID
    ///
    /// # 返回
    /// 返回完整任务视图（含进度与错误摘要）。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    pub async fn background_job_detail(&self, id: &str) -> Result<BackgroundJobView> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("后台任务不存在".to_string()))?;
        Ok(job.into())
    }

    /// 创建后台任务并登记逐项结果表（跨集合事务写入，按 `request_id` 幂等）。
    ///
    /// 同一 `request_id` 重复提交时返回既有任务（幂等命中），不产生第二条
    /// 任务；唯一约束由 `uk_background_jobs_request_id` 承担。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回任务视图（新建或幂等命中的既有任务）。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - `job_no`/`request_id` 与既有任务冲突（唯一索引透出）
    pub async fn create_background_job(
        &self,
        req: CreateBackgroundJobRequest,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        req.validate()?;
        if let Some(existing) = self
            .db
            .background_jobs()
            .find_by_request_id(&req.request_id, &mut NoTransaction)
            .await?
        {
            return Ok(existing.into());
        }
        let job_id = BackgroundJobId::new(next_id());
        let job = BackgroundJob::new(
            job_id.clone(),
            BackgroundJobData {
                job_no: req.job_no,
                job_type: req.job_type,
                domain_job_type: req.domain_job_type,
                domain_job_id: req.domain_job_id,
                selection_snapshot_id: req.selection_snapshot_id.map(BulkSelectionSnapshotId::new),
                requested_by: actor.id().to_string(),
                request_id: req.request_id,
                input_file_asset_id: req.input_file_asset_id.map(FileAssetId::new),
                result_file_asset_id: None,
                total_count: req.total_count,
            },
        )?;
        let items = build_job_items(&job_id, req.items)?;
        let audit =
            actor
                .clone()
                .resource_log("background_job.create", "background_job", job.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let job_for_tx = job.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.bulk_job()
                        .create_job_with_items(&job_for_tx, items, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(job.into())
    }

    /// 取消后台任务。
    ///
    /// 任务取消只停止尚未开始的项目；已经提交的正式事实不回滚、不删除（§6.1）。
    ///
    /// # 参数
    /// * `id` - 后台任务 ID
    /// * `req` - 取消请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回取消后的任务视图。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `ConflictError` - 版本陈旧或任务已处于终态
    pub async fn cancel_background_job(
        &self,
        id: &str,
        req: CancelBackgroundJobRequest,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        req.validate()?;
        let mut job = self.load_job_with_version(id, req.version).await?;
        job.cancel(entities::common::time::Instant::now())?;
        let audit =
            actor
                .clone()
                .resource_log("background_job.cancel", "background_job", job.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.background_jobs().update(&mut job, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<BackgroundJob, crate::errors::Error>(job)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 分页查询任务逐项结果。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    /// * `status` - 逐项执行结果筛选
    /// * `page` - 页码（1 起）
    /// * `page_size` - 单页条数
    ///
    /// # 返回
    /// 返回当前页逐项结果行与总数。
    ///
    /// # 错误
    /// 分页参数非法或数据库查询失败时返回错误。
    pub async fn background_job_item_list(
        &self,
        job_id: &str,
        status: Option<entities::bulk_job::ItemStatus>,
        page: u64,
        page_size: u32,
    ) -> Result<PageView<BackgroundJobItemView>> {
        if page == 0 {
            return Err(Error::ValidationError("页码必须大于0".to_string()));
        }
        let job_id = BackgroundJobId::new(job_id);
        let result = self
            .db
            .background_job_items()
            .search_job_items(&job_id, status, page, page_size, &mut NoTransaction)
            .await?;
        let items = result
            .items
            .into_iter()
            .map(|row| BackgroundJobItemView {
                id: row.id,
                background_job_id: job_id.to_string(),
                item_no: row.item_no,
                object_type: row.object_type,
                object_id: row.object_id,
                status: row.status,
                result_code: row.result_code,
                result_summary: row.result_summary,
                result_object_type: row.result_object_type,
                result_object_id: row.result_object_id,
            })
            .collect();

        Ok(PageView {
            items,
            total: result.total,
            page,
            page_size,
        })
    }

    /// 校验批量冻结目标中业务单据类对象已注册（跨域 D02 仓储读取）。
    ///
    /// # 参数
    /// * `items` - 冻结目标请求
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 目标命中 `DocumentType` 目录但单据未注册时返回 `NotFound`。
    async fn ensure_items_registered(&self, items: &[CreateBulkSelectionItemRequest]) -> Result<()> {
        for item in items {
            if is_business_document_type(&item.object_type) {
                self.db
                    .business_documents()
                    .find_by_id(&item.object_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
            }
        }
        Ok(())
    }

    /// 按 ID 加载快照并校验期望版本。
    ///
    /// # 参数
    /// * `id` - 快照 ID
    /// * `expected_version` - 请求携带的期望版本
    ///
    /// # 返回
    /// 返回加载的快照实体。
    ///
    /// # 错误
    /// 快照不存在返回 `NotFound`；版本不一致返回 `ConflictError`。
    async fn load_snapshot_with_version(
        &self,
        id: &str,
        expected_version: u64,
    ) -> Result<BulkSelectionSnapshot> {
        let snapshot = self
            .db
            .bulk_selection_snapshots()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("选择快照不存在".to_string()))?;
        if snapshot.base.version != expected_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(snapshot)
    }

    /// 在单个事务中更新快照并追加审计日志。
    ///
    /// # 参数
    /// * `mut snapshot` - 已由实体完成状态迁移的快照
    /// * `action` - 审计动作名
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的快照视图。
    ///
    /// # 错误
    /// 并发写入冲突或审计写入失败时返回错误。
    async fn update_snapshot_with_audit(
        &self,
        mut snapshot: BulkSelectionSnapshot,
        action: &str,
        actor: &AuditActor,
    ) -> Result<BulkSelectionSnapshotView> {
        let audit =
            actor
                .clone()
                .resource_log(action, "bulk_selection_snapshot", snapshot.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.bulk_selection_snapshots()
                        .update(&mut snapshot, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<BulkSelectionSnapshot, crate::errors::Error>(snapshot)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 按 ID 加载后台任务并校验期望版本。
    ///
    /// # 参数
    /// * `id` - 后台任务 ID
    /// * `expected_version` - 请求携带的期望版本
    ///
    /// # 返回
    /// 返回加载的任务实体。
    ///
    /// # 错误
    /// 任务不存在返回 `NotFound`；版本不一致返回 `ConflictError`。
    async fn load_job_with_version(&self, id: &str, expected_version: u64) -> Result<BackgroundJob> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("后台任务不存在".to_string()))?;
        if job.base.version != expected_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(job)
    }
}

/// 构建选择快照冻结目标（`selection_snapshot_id` 统一注入）。
///
/// # 参数
/// * `snapshot_id` - 选择快照 ID
/// * `items` - 冻结目标请求
///
/// # 返回
/// 返回实体层冻结目标集合。
///
/// # 错误
/// 任意一项实体校验失败时返回错误。
fn build_selection_items(
    snapshot_id: &BulkSelectionSnapshotId,
    items: Vec<CreateBulkSelectionItemRequest>,
) -> Result<Vec<BulkSelectionItem>> {
    let mut built = Vec::with_capacity(items.len());
    for item in items {
        let selection_item = BulkSelectionItem::new(
            entities::ids::BulkSelectionItemId::new(next_id()),
            BulkSelectionItemData {
                selection_snapshot_id: snapshot_id.clone(),
                object_type: item.object_type,
                object_id: item.object_id,
                expected_version: item.expected_version,
                expected_hash: item.expected_hash,
            },
        )?;
        built.push(selection_item);
    }
    Ok(built)
}

/// 构建后台任务逐项结果行（`item_no` 从 1 递增）。
///
/// # 参数
/// * `job_id` - 后台任务 ID
/// * `items` - 逐项请求
///
/// # 返回
/// 返回实体层逐项结果行。
///
/// # 错误
/// 任意一行实体校验失败时返回错误。
fn build_job_items(
    job_id: &BackgroundJobId,
    items: Vec<CreateBackgroundJobItemRequest>,
) -> Result<Vec<BackgroundJobItem>> {
    let mut built = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let job_item = BackgroundJobItem::new(
            entities::ids::BackgroundJobItemId::new(next_id()),
            BackgroundJobItemData {
                background_job_id: job_id.clone(),
                item_no: index as u32 + 1,
                object_type: item.object_type,
                object_id: item.object_id,
                expected_version: item.expected_version,
                expected_hash: item.expected_hash,
                worksheet_name: item.worksheet_name,
                source_row_no: item.source_row_no,
                source_column_name: item.source_column_name,
            },
        )?;
        built.push(job_item);
    }
    Ok(built)
}

/// 判断对象类型代码是否属于业务单据目录（判定来自 entities 的 serde 目录）。
///
/// # 参数
/// * `object_type` - 目标对象类型代码
///
/// # 返回
/// 命中 `DocumentType` 任一变体时返回 `true`。
fn is_business_document_type(object_type: &str) -> bool {
    use serde::de::{
        value::{Error as SerdeError, StrDeserializer},
        IntoDeserializer,
    };
    let deserializer: StrDeserializer<SerdeError> = object_type.into_deserializer();
    DocumentType::deserialize(deserializer).is_ok()
}
