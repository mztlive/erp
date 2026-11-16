//! 域 D04 `bulk_job` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建选择快照 + 冻结目标、创建后台任务 + 逐项结果：跨集合多步骤写入 →
//!   `with_transaction`（复用域专用仓储 `create_snapshot_with_items` /
//!   `create_job_with_items`）+ 审计日志同事务；
//! - 确认/失效/取消：业务行 + 审计日志 → `with_transaction`；
//! - 受保护查询的功能资格与业务读取共用同一快照事务。
//!
//! 幂等：后台任务按 `request_id` 唯一索引幂等，重复提交返回既有任务
//! （§6.1：涉及资金/状态机变更的入口必须有幂等键）。
//!
//! 跨域：批量冻结目标命中业务单据类型目录时，经消费方 Port 校验单据已注册。

mod cancel_all;

use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{BulkSelectionItemId, FileAssetId};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, bulk_job};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::check_expected_version;
use crate::authorization::background::BackgroundAuthorization;
pub use crate::dto::bulk_job::{
    BackgroundJobItemView, BackgroundJobListParams, BackgroundJobView, BulkSelectionItemView,
    BulkSelectionSnapshotListParams, BulkSelectionSnapshotView, CancelAllBackgroundJobsRequest,
    CancelAllBackgroundJobsResponse, CancelBackgroundJobFailure, CancelBackgroundJobRequest,
    ConfirmBulkSelectionSnapshotRequest, CreateBackgroundJobItemRequest, CreateBackgroundJobRequest,
    CreateBulkSelectionItemRequest, CreateBulkSelectionSnapshotRequest, ExpireBulkSelectionSnapshotRequest,
    PageView,
};
use crate::entity::bulk_job::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobId, BackgroundJobItem,
    BackgroundJobItemDraft, BulkSelectionItem, BulkSelectionItemDraft, BulkSelectionSnapshot,
    BulkSelectionSnapshotAggregate, BulkSelectionSnapshotAggregateData, BulkSelectionSnapshotId,
};
use crate::error::{Error, Result};
use crate::ports::{BusinessDocumentPort, SupportAuditPort, is_business_document_type};
use crate::repository::bulk_job::authorized_background_page;
use crate::repository::prelude::*;
use crate::repository::{BackgroundJobRegistration, BulkJobExt};

/// 选择快照列表筛选条件类型（经 `BulkJobExt` 关联类型跨 crate 可达）。
type BulkSelectionSnapshotFilter = <mongodb::Database as BulkJobExt>::BulkSelectionSnapshotFilter;
/// 后台任务列表筛选条件类型。
type BackgroundJobFilter = <mongodb::Database as BulkJobExt>::BackgroundJobFilter;

/// 批量任务服务。
///
/// 提供选择快照与后台任务的创建、确认与查询编排。
#[derive(Clone)]
pub struct BulkJobService {
    db: Database,
    audit: Arc<dyn SupportAuditPort>,
    documents: Arc<dyn BusinessDocumentPort>,
    authorization: BackgroundAuthorization,
}

impl BulkJobService {
    /// 创建批量任务服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `documents` - 业务单据注册端口
    /// * `authorization` - 完成启动绑定的共享授权服务。
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        audit: Arc<dyn SupportAuditPort>,
        documents: Arc<dyn BusinessDocumentPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        let authorization = BackgroundAuthorization::new(authorization);
        Self { db, audit, documents, authorization }
    }

    /// 分页查询选择快照列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`selection_type`/`status`/`created_by` 扁平筛选）
    /// * `actor` - 当前认证账号，由共享服务检查列表资格。
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
        actor: &AuditActor,
    ) -> Result<PageView<BulkSelectionSnapshotView>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.selection_snapshot_page(&params, &actor, executor).await })
            })
            .await
    }

    /// 选择快照列表资格与完整分页共用原快照事务。
    async fn selection_snapshot_page(
        &self,
        params: &BulkSelectionSnapshotListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<BulkSelectionSnapshotView>> {
        self.authorization
            .service
            .require(actor, &bulk_job::BULK_SELECTION_SNAPSHOT_LIST, AccessTarget::Operation, executor)
            .await?;
        params.validate()?;
        let query = params.normalized()?;
        let (page, page_size, sort_by, sort_ascending) = query.paging.into_filter_parts();
        let filter = BulkSelectionSnapshotFilter {
            selection_type: query.selection_type,
            status: query.status,
            created_by: query.created_by,
            page,
            page_size,
            sort_by,
            sort_ascending,
        };
        let page = self.db.bulk_selection_snapshots().search_snapshots(&filter, executor).await?;
        let items = page.items.into_iter().map(BulkSelectionSnapshotView::from).collect();

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
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
        let snapshot_id = BulkSelectionSnapshotId::new(next_id());
        let drafts = req
            .items
            .into_iter()
            .map(|item| BulkSelectionItemDraft {
                id: BulkSelectionItemId::new(next_id()),
                object_type: item.object_type,
                object_id: item.object_id,
                expected_version: item.expected_version,
                expected_hash: item.expected_hash,
            })
            .collect();
        let cutoff = i64::try_from(req.data_cutoff_at)
            .map_err(|_| Error::ValidationError("数据截止时间超出支持范围".into()))?;
        let expires = i64::try_from(req.expires_at)
            .map_err(|_| Error::ValidationError("快照过期时间超出支持范围".into()))?;
        let aggregate = BulkSelectionSnapshotAggregate::new(
            snapshot_id,
            BulkSelectionSnapshotAggregateData {
                selection_type: req.selection_type,
                data_cutoff_at: Instant::from_unix_secs(cutoff),
                created_by: actor.id().to_string(),
                expires_at: Instant::from_unix_secs(expires),
            },
            drafts,
        )?;
        let (snapshot, items) = aggregate.into_parts();
        let service = self.clone();
        let actor = actor.clone();
        let snapshot = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.persist_snapshot(snapshot, items, &actor, executor).await })
            })
            .await?;

        Ok(snapshot.into())
    }

    /// 在创建事务中完成资格、真实目标注册检查和快照原子写入。
    async fn persist_snapshot(
        &self,
        snapshot: BulkSelectionSnapshot,
        items: Vec<BulkSelectionItem>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BulkSelectionSnapshot> {
        self.authorization
            .service
            .require(actor, &bulk_job::BULK_SELECTION_SNAPSHOT_CREATE, AccessTarget::Operation, executor)
            .await?;
        Self::ensure_items_registered(self.documents.as_ref(), &items, executor).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "bulk_selection_snapshot.create",
            "bulk_selection_snapshot",
            snapshot.base.id.clone(),
        )?;
        self.db.bulk_job().create_snapshot_with_items(&snapshot, items, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(snapshot)
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
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .change_snapshot(
                            &id,
                            req.version,
                            bulk_job::BULK_SELECTION_SNAPSHOT_CONFIRM,
                            &actor,
                            executor,
                        )
                        .await
                })
            })
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
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .change_snapshot(
                            &id,
                            req.version,
                            bulk_job::BULK_SELECTION_SNAPSHOT_EXPIRE,
                            &actor,
                            executor,
                        )
                        .await
                })
            })
            .await
    }

    /// 分页查询快照逐项结果。
    ///
    /// # 参数
    /// * `snapshot_id` - 选择快照 ID
    /// * `result_status` - 逐项执行结果筛选
    /// * `page` - 页码（1 起）
    /// * `page_size` - 单页条数
    /// * `actor` - 当前认证账号，由共享服务检查逐项读取资格。
    ///
    /// # 返回
    /// 返回当前页逐项结果行与总数。
    ///
    /// # 错误
    /// 分页参数非法或数据库查询失败时返回错误。
    pub async fn bulk_selection_item_list(
        &self,
        snapshot_id: &str,
        result_status: Option<crate::entity::bulk_job::SelectionItemStatus>,
        page: u64,
        page_size: u32,
        actor: &AuditActor,
    ) -> Result<PageView<BulkSelectionItemView>> {
        let service = self.clone();
        let snapshot_id = snapshot_id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .selection_item_page(&snapshot_id, result_status, page, page_size, &actor, executor)
                        .await
                })
            })
            .await
    }

    /// 选择项读取资格、分页与计数共用原快照事务。
    async fn selection_item_page(
        &self,
        snapshot_id: &str,
        result_status: Option<crate::entity::bulk_job::SelectionItemStatus>,
        page: u64,
        page_size: u32,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<BulkSelectionItemView>> {
        self.authorization
            .service
            .require(actor, &bulk_job::BULK_SELECTION_ITEM_LIST, AccessTarget::Operation, executor)
            .await?;
        if page == 0 {
            return Err(Error::ValidationError("页码必须大于0".to_string()));
        }
        let snapshot_id = BulkSelectionSnapshotId::new(snapshot_id);
        let result = self
            .db
            .bulk_selection_items()
            .search_items(&snapshot_id, result_status, page, page_size, executor)
            .await?;
        let items = result.items.into_iter().map(BulkSelectionItemView::from).collect();

        Ok(PageView { items, total: result.total, page, page_size })
    }

    /// 分页查询后台任务列表（任务中心）。
    ///
    /// identity 统一执行本人或启用管理员范围；requested_by 仅收窄结果。
    ///
    /// # 参数
    /// * `params` - 查询参数（`job_no`/`job_type`/`status`/`requested_by` 扁平筛选）
    /// * `actor` - 已通过鉴权的审计操作人
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
        actor: &AuditActor,
    ) -> Result<PageView<BackgroundJobView>> {
        params.validate()?;
        let filter = BackgroundJobFilter::from(params.normalized()?);
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        let db = self.db.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = authorization.query(&actor, executor).await?;
                    let page = authorized_background_page(&db, &filter, &access, executor).await?;
                    authorization.service.validate(&access, executor).await?;
                    Ok(PageView {
                        items: page.items.into_iter().map(Into::into).collect(),
                        total: page.total,
                        page: filter.page,
                        page_size: filter.page_size,
                    })
                })
            })
            .await
    }

    /// 查询认证入口的本人任务；该入口也服务供应商门户，不授予后台管理资格。
    /// # 参数
    /// params 为分页条件，actor 为原入口认证的当前账号。
    /// # 返回
    /// 固定发起人为当前账号的任务页，忽略请求中的其他发起人。
    /// # 错误
    /// 分页、筛选或数据库读取失败时返回错误。
    pub async fn own_background_job_list(
        &self,
        params: &BackgroundJobListParams,
        actor: &AuditActor,
    ) -> Result<PageView<BackgroundJobView>> {
        params.validate()?;
        let filter = BackgroundJobFilter::for_requester(params.normalized()?, actor.id());
        let page = self.db.background_jobs().search_background_jobs(&filter, &mut NoTransaction).await?;
        Ok(PageView {
            items: page.items.into_iter().map(Into::into).collect(),
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询后台任务详情。
    ///
    /// 数据隔离：普通用户只能查看自己创建的任务。
    ///
    /// # 参数
    /// * `id` - 后台任务 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回完整任务视图（含进度与错误摘要）。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `Forbidden` - 非管理员查看他人创建的任务
    pub async fn background_job_detail(&self, id: &str, actor: &AuditActor) -> Result<BackgroundJobView> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let job = service
                        .authorized_job(&id, &bulk_job::BACKGROUND_JOB_DETAIL, &actor, executor)
                        .await?;
                    Ok(job.into())
                })
            })
            .await
    }

    /// 先检查完整任务资格，再从原快照读取业务对象；拒绝时不探测存在性。
    async fn authorized_job(
        &self,
        id: &str,
        permission: &Permission,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BackgroundJob> {
        self.authorization.require(permission, id, actor, executor).await?;
        self.db
            .background_jobs()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("后台任务不存在".into()))
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
        let (job, items) = build_job_parts(req, actor)?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "background_job.create",
            "background_job",
            job.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let job_for_tx = job.clone();
        let audit_port = self.audit.clone();
        let authorization = self.authorization.service.clone();
        let transaction_actor = actor.clone();
        let created = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(
                            &transaction_actor,
                            &bulk_job::BACKGROUND_JOB_CREATE,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    db.bulk_job()
                        .create_job_with_items(&job_for_tx, items, executor)
                        .await
                        .map_err(Error::RepositoryError)?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await;

        match created {
            Ok(()) => Ok(job.into()),
            Err(Error::RepositoryError(persistence_core::Error::DuplicateKey(_))) => {
                self.resolve_creation_race(&job, actor).await
            },
            Err(Error::RepositoryError(error)) => Err(error.into()),
            Err(error) => Err(error),
        }
    }

    /// 唯一键竞争后按 `request_id` 回查仲裁结果（幂等恢复路径）。
    ///
    /// # 参数
    /// * `job` - 本次尝试创建的任务（含 `request_id` 幂等身份）
    ///
    /// # 返回
    /// 幂等命中的既有任务视图。
    ///
    /// # 错误
    /// * `ConflictError` - 请求身份已被不同载荷占用，或竞争结果已变化需刷新重试
    /// * `RepositoryError` - 回查仲裁结果时数据库查询失败
    async fn resolve_creation_race(
        &self,
        job: &BackgroundJob,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        let (service, job, actor) = (self.clone(), job.clone(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.creation_replay(&job, &actor, executor).await })
            })
            .await
    }

    /// 在新快照核验原创建资格，再按原身份只读查证，不重放写入。
    async fn creation_replay(
        &self,
        job: &BackgroundJob,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BackgroundJobView> {
        self.authorization
            .service
            .require(actor, &bulk_job::BACKGROUND_JOB_CREATE, AccessTarget::Operation, executor)
            .await?;
        match self.db.bulk_job().registration_by_request_id(job, executor).await? {
            Some(BackgroundJobRegistration::ReplaySame(existing)) => Ok(existing.into()),
            Some(BackgroundJobRegistration::ConflictDifferentPayload(_)) => Err(Error::ConflictError(
                "同一请求身份已用于不同后台任务载荷；历史无指纹任务须使用新的请求身份".to_string(),
            )),
            Some(BackgroundJobRegistration::Created) | None => {
                Err(Error::ConflictError("后台任务唯一竞争结果已变化，请刷新后重试".to_string()))
            },
        }
    }

    /// 取消后台任务。
    ///
    /// 任务取消只停止尚未开始的项目；已经提交的正式事实不回滚、不删除（§6.1）。
    /// 数据隔离：普通用户只能取消自己创建的任务。
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
    /// * `Forbidden` - 非管理员取消他人创建的任务
    /// * `ConflictError` - 版本陈旧或任务已处于终态
    pub async fn cancel_background_job(
        &self,
        id: &str,
        req: CancelBackgroundJobRequest,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        req.validate()?;
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut job = service
                        .authorized_job(&id, &bulk_job::BACKGROUND_JOB_CANCEL, &actor, executor)
                        .await?;
                    check_expected_version(job.base.version, req.version)?;
                    job.cancel(Instant::now())?;
                    let audit =
                        service.audit.resource_log(actor, "background_job.cancel", "background_job", id)?;
                    service.db.background_jobs().update(&mut job, executor).await?;
                    service.audit.persist(&audit, executor).await?;
                    Ok(job.into())
                })
            })
            .await
    }

    /// 分页查询任务逐项结果。
    ///
    /// 数据隔离：普通用户只能查看自己创建任务的逐项结果。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    /// * `status` - 逐项执行结果筛选
    /// * `page` - 页码（1 起）
    /// * `page_size` - 单页条数
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回当前页逐项结果行与总数。
    ///
    /// # 错误
    /// 分页参数非法、任务不存在、无权查看或数据库查询失败时返回错误。
    pub async fn background_job_item_list(
        &self,
        job_id: &str,
        status: Option<crate::entity::bulk_job::ItemStatus>,
        page: u64,
        page_size: u32,
        actor: &AuditActor,
    ) -> Result<PageView<BackgroundJobItemView>> {
        if page == 0 {
            return Err(Error::ValidationError("页码必须大于0".into()));
        }
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let job_id = job_id.to_owned();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(&bulk_job::BACKGROUND_JOB_ITEM_LIST, &job_id, &actor, executor)
                        .await
                        .map_err(|error| match error {
                            Error::NotFound(_) => Error::NotFound("后台任务不存在".into()),
                            error => error,
                        })?;
                    let result = db
                        .background_job_items()
                        .search_job_items(
                            &BackgroundJobId::new(job_id.clone()),
                            status,
                            page,
                            page_size,
                            executor,
                        )
                        .await?;
                    Ok(PageView {
                        items: result.items.into_iter().map(Into::into).collect(),
                        total: result.total,
                        page,
                        page_size,
                    })
                })
            })
            .await
    }

    /// 校验批量冻结目标中业务单据类对象已注册（跨域 D02 仓储读取）。
    ///
    /// # 参数
    /// * `items` - 待保存的冻结目标实体
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 目标命中 `DocumentType` 目录但单据未注册时返回 `NotFound`。
    async fn ensure_items_registered(
        documents: &dyn BusinessDocumentPort,
        items: &[BulkSelectionItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let document_ids = items
            .iter()
            .filter(|item| is_business_document_type(&item.object_type))
            .map(|item| item.object_id.clone())
            .collect::<Vec<_>>();
        let unique_ids = dedup_preserving_order(&document_ids);
        let registered_ids =
            documents.find_registered_ids(&unique_ids, executor).await?.into_iter().collect::<HashSet<_>>();
        if first_unregistered_document_id(&document_ids, &registered_ids).is_some() {
            return Err(Error::NotFound("业务单据未注册".to_string()));
        }
        Ok(())
    }

    /// 在授权事务中读取、校验版本并迁移选择快照。
    async fn change_snapshot(
        &self,
        id: &str,
        version: u64,
        permission: Permission,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BulkSelectionSnapshotView> {
        self.authorization.service.require(actor, &permission, AccessTarget::Operation, executor).await?;
        let mut snapshot = self
            .db
            .bulk_selection_snapshots()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选择快照不存在".into()))?;
        check_expected_version(snapshot.base.version, version)?;
        match permission.action() {
            "confirm" => snapshot.confirm()?,
            "expire" => snapshot.expire()?,
            _ => return Err(Error::ValidationError("未知选择快照动作".into())),
        }
        let audit = self.audit.resource_log(
            actor.clone(),
            &format!("bulk_selection_snapshot.{}", permission.action()),
            "bulk_selection_snapshot",
            id.to_owned(),
        )?;
        self.db.bulk_selection_snapshots().update(&mut snapshot, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(snapshot.into())
    }
}

/// 由创建请求组装后台任务聚合并拆分为父实体与逐项行。
///
/// # 参数
/// * `req` - 创建请求（含逐项结果与声明总数）
/// * `actor` - 已通过鉴权的审计操作人（记为发起人）
///
/// # 返回
/// 返回待写入事务的任务实体与逐项行。
///
/// # 错误
/// 请求体校验失败或聚合不变量（空目标、总数不一致）失败时返回错误。
fn build_job_parts(
    req: CreateBackgroundJobRequest,
    actor: &AuditActor,
) -> Result<(BackgroundJob, Vec<BackgroundJobItem>)> {
    let job_id = BackgroundJobId::new(next_id());
    let drafts = req
        .items
        .into_iter()
        .map(|item| BackgroundJobItemDraft {
            id: erp_core::ids::BackgroundJobItemId::new(next_id()),
            object_type: item.object_type,
            object_id: item.object_id,
            expected_version: item.expected_version,
            expected_hash: item.expected_hash,
            worksheet_name: item.worksheet_name,
            source_row_no: item.source_row_no,
            source_column_name: item.source_column_name,
        })
        .collect();
    let aggregate = BackgroundJobAggregate::new(
        job_id,
        BackgroundJobAggregateData {
            job_no: req.job_no,
            job_type: req.job_type,
            domain_job_type: req.domain_job_type,
            domain_job_id: req.domain_job_id,
            selection_snapshot_id: req.selection_snapshot_id.map(BulkSelectionSnapshotId::new),
            requested_by: actor.id().to_string(),
            request_id: req.request_id,
            input_file_asset_id: req.input_file_asset_id.map(FileAssetId::new),
            result_file_asset_id: None,
            declared_total_count: req.total_count,
        },
        drafts,
    )?;
    Ok(aggregate.into_parts())
}

/// 借用去重键，只复制首次出现的 ID。
///
/// # 参数
/// * `ids` - 待去重的 ID 序列
///
/// # 返回
/// 返回保序去重后的 ID 集合；去重语义与 `NotFound` 行为不变。
fn dedup_preserving_order(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter().filter(|id| seen.insert(id.as_str())).cloned().collect()
}

/// 按请求顺序定位首个尚未注册的业务单据 ID。
///
/// # 参数
/// * `document_ids` - 从请求提取的业务单据 ID，保留原始顺序与重复项
/// * `registered_ids` - 仓储批量查询返回的已注册 ID 集合
///
/// # 返回
/// 全部已注册时返回 `None`；否则返回首个缺失 ID。
///
/// # 错误
/// 无。
fn first_unregistered_document_id<'a>(
    document_ids: &'a [String],
    registered_ids: &HashSet<String>,
) -> Option<&'a str> {
    document_ids.iter().find(|id| !registered_ids.contains(id.as_str())).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::first_unregistered_document_id;

    #[tokio::test]
    async fn detail_and_cancel_authorize_before_reading_job_existence() {
        use std::sync::Arc;

        use application_core::AuditActor;
        use erp_core::AccountKind;
        use erp_identity::testing::unavailable_authorization;
        use erp_permissions::bulk_job;
        use persistence_core::testing::SnapshotExecutor;

        use super::BulkJobService;
        use crate::error::Error;
        use crate::ports::{FailClosedAuditPort, FailClosedBusinessDocumentPort};

        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service = BulkJobService::new(
            client.database("background_job_original_authorization"),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedBusinessDocumentPort),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin);
        for permission in [bulk_job::BACKGROUND_JOB_DETAIL, bulk_job::BACKGROUND_JOB_CANCEL] {
            let error = service
                .authorized_job("unread-job", &permission, &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("资格取证失败不得读取任务存在性或版本");
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
    }

    #[tokio::test]
    async fn creation_race_replay_rechecks_original_qualification_before_reading_registered_job() {
        use std::sync::Arc;

        use application_core::AuditActor;
        use erp_core::AccountKind;
        use erp_identity::testing::unavailable_authorization;
        use persistence_core::NoTransaction;
        use persistence_core::testing::SnapshotExecutor;

        use super::*;
        use crate::entity::bulk_job::{BackgroundJobData, JobType};
        use crate::ports::{FailClosedAuditPort, FailClosedBusinessDocumentPort};

        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service = BulkJobService::new(
            client.database("creation_race_authorization"),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedBusinessDocumentPort),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin);
        let job = BackgroundJob::new(
            BackgroundJobId::new("new-job"),
            BackgroundJobData {
                job_no: "JOB-1".into(),
                job_type: JobType::Import,
                domain_job_type: None,
                domain_job_id: None,
                selection_snapshot_id: None,
                requested_by: actor.id().into(),
                request_id: "request".into(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 1,
            },
        )
        .unwrap();
        let error = service
            .creation_replay(&job, &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("原创建资格失效不得返回竞争结果");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        let error = service
            .creation_replay(&job, &actor, &mut NoTransaction)
            .await
            .expect_err("幂等回放必须使用新快照");
        assert!(matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务"));
    }

    /// 选择快照的两个公开读取入口必须先通过共享授权，再访问数据库。
    #[tokio::test]
    async fn selection_reads_require_identity_before_loading_rows() {
        use std::sync::Arc;

        use application_core::AuditActor;
        use erp_core::AccountKind;
        use erp_identity::testing::unavailable_authorization;
        use persistence_core::testing::SnapshotExecutor;

        use super::{BulkJobService, BulkSelectionSnapshotListParams};
        use crate::error::Error;
        use crate::ports::{FailClosedAuditPort, FailClosedBusinessDocumentPort};

        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service = BulkJobService::new(
            client.database("selection_authorization_test"),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedBusinessDocumentPort),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let errors = [
            service
                .selection_snapshot_page(
                    &BulkSelectionSnapshotListParams::default(),
                    &actor,
                    &mut SnapshotExecutor::default(),
                )
                .await
                .expect_err("必须拒绝"),
            service
                .selection_item_page("snapshot", None, 1, 20, &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("必须拒绝"),
        ];
        for error in errors {
            assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
        }
    }

    #[test]
    fn registration_membership_preserves_first_missing_request_order() {
        let ids = vec![
            "registered".to_string(),
            "missing-first".to_string(),
            "registered".to_string(),
            "missing-later".to_string(),
        ];
        let registered = HashSet::from(["registered".to_string()]);
        assert_eq!(first_unregistered_document_id(&ids, &registered), Some("missing-first"));

        let all_registered = HashSet::from([
            "registered".to_string(),
            "missing-first".to_string(),
            "missing-later".to_string(),
        ]);
        assert_eq!(first_unregistered_document_id(&ids, &all_registered), None);
        assert_eq!(first_unregistered_document_id(&[], &HashSet::new()), None);
    }
}
