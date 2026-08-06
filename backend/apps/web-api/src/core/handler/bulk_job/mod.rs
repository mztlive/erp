//! 域 D04 `bulk_job` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::bulk_job` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    bulk_job::{
        BackgroundJobItemView, BackgroundJobListParams, BackgroundJobView, BulkJobService,
        BulkSelectionItemView, BulkSelectionSnapshotListParams, BulkSelectionSnapshotView,
        CancelBackgroundJobRequest, ConfirmBulkSelectionSnapshotRequest, CreateBackgroundJobRequest,
        CreateBulkSelectionSnapshotRequest, ExpireBulkSelectionSnapshotRequest, PageView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "查询选择快照列表",
    resource = "bulk_selection_snapshot",
    action = "list"
)]
/// 查询选择快照列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`selection_type`/`status`/`created_by`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn bulk_selection_snapshot_list(
    State(state): State<AppState>,
    Query(params): Query<BulkSelectionSnapshotListParams>,
) -> Result<PageView<BulkSelectionSnapshotView>> {
    let page = BulkJobService::new(state.db())
        .bulk_selection_snapshot_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "创建选择快照",
    resource = "bulk_selection_snapshot",
    action = "create"
)]
/// 创建选择快照并冻结逐项目标（跨集合事务写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ selection_type, data_cutoff_at, expires_at, items }`）
///
/// # 返回
/// 返回新建的快照视图。
pub async fn bulk_selection_snapshot_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateBulkSelectionSnapshotRequest>,
) -> Result<BulkSelectionSnapshotView> {
    let view = BulkJobService::new(state.db())
        .create_bulk_selection_snapshot(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "确认选择快照",
    resource = "bulk_selection_snapshot",
    action = "confirm"
)]
/// 确认选择快照（确认后目标集合与预期版本不可修改）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 快照 ID
/// * `req` - 确认请求（含期望版本）
///
/// # 返回
/// 返回确认后的快照视图。
pub async fn bulk_selection_snapshot_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ConfirmBulkSelectionSnapshotRequest>,
) -> Result<BulkSelectionSnapshotView> {
    let view = BulkJobService::new(state.db())
        .confirm_bulk_selection_snapshot(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "失效选择快照",
    resource = "bulk_selection_snapshot",
    action = "expire"
)]
/// 标记选择快照失效（仅待确认/已确认可失效）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 快照 ID
/// * `req` - 失效请求（含期望版本）
///
/// # 返回
/// 返回失效后的快照视图。
pub async fn bulk_selection_snapshot_expire(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ExpireBulkSelectionSnapshotRequest>,
) -> Result<BulkSelectionSnapshotView> {
    let view = BulkJobService::new(state.db())
        .expire_bulk_selection_snapshot(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "查询选择快照逐项结果",
    resource = "bulk_selection_item",
    action = "list"
)]
/// 分页查询选择快照逐项结果。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 选择快照 ID
/// * `query` - 分页参数（`result_status`/`page`/`page_size`）
///
/// # 返回
/// 返回当前页逐项结果行与总数。
pub async fn bulk_selection_item_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<BulkSelectionItemListQuery>,
) -> Result<PageView<BulkSelectionItemView>> {
    let page = BulkJobService::new(state.db())
        .bulk_selection_item_list(&id, query.result_status, query.page, query.page_size)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "查询后台任务列表",
    resource = "background_job",
    action = "list"
)]
/// 查询后台任务列表（任务中心）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`job_no`/`job_type`/`status`/`requested_by`）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn background_job_list(
    State(state): State<AppState>,
    Query(params): Query<BackgroundJobListParams>,
) -> Result<PageView<BackgroundJobView>> {
    let page = BulkJobService::new(state.db())
        .background_job_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "查询后台任务详情",
    resource = "background_job",
    action = "detail"
)]
/// 查询后台任务详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 后台任务 ID
///
/// # 返回
/// 返回完整任务视图（含进度与错误摘要）。
pub async fn background_job_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<BackgroundJobView> {
    let view = BulkJobService::new(state.db()).background_job_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "创建后台任务",
    resource = "background_job",
    action = "create"
)]
/// 创建后台任务并登记逐项结果表（按 `request_id` 幂等）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ job_no, job_type, request_id, total_count, items }`）
///
/// # 返回
/// 返回任务视图（新建或幂等命中的既有任务）。
pub async fn background_job_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateBackgroundJobRequest>,
) -> Result<BackgroundJobView> {
    let view = BulkJobService::new(state.db())
        .create_background_job(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "取消后台任务",
    resource = "background_job",
    action = "cancel"
)]
/// 取消后台任务（只停止尚未开始的项目）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 后台任务 ID
/// * `req` - 取消请求（含期望版本）
///
/// # 返回
/// 返回取消后的任务视图。
pub async fn background_job_cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelBackgroundJobRequest>,
) -> Result<BackgroundJobView> {
    let view = BulkJobService::new(state.db())
        .cancel_background_job(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "批量任务",
    group_desc = "批量选择快照与后台任务中心",
    desc = "查询后台任务逐项结果",
    resource = "background_job_item",
    action = "list"
)]
/// 分页查询后台任务逐项结果。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 后台任务 ID
/// * `query` - 分页参数（`status`/`page`/`page_size`）
///
/// # 返回
/// 返回当前页逐项结果行与总数。
pub async fn background_job_item_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<BackgroundJobItemListQuery>,
) -> Result<PageView<BackgroundJobItemView>> {
    let page = BulkJobService::new(state.db())
        .background_job_item_list(&id, query.status, query.page, query.page_size)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 选择快照逐项列表查询参数。
///
/// HTTP 形态差异说明：Service 方法接收拆开的 `(result_status, page, page_size)`，
/// 此处是 Query 提取所需的最薄包装。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BulkSelectionItemListQuery {
    /// 逐项执行结果筛选。
    pub result_status: Option<entities::bulk_job::SelectionItemStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}

/// 后台任务逐项列表查询参数。
///
/// HTTP 形态差异说明：Service 方法接收拆开的 `(status, page, page_size)`，
/// 此处是 Query 提取所需的最薄包装。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BackgroundJobItemListQuery {
    /// 逐项执行结果筛选。
    pub status: Option<entities::bulk_job::ItemStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}
