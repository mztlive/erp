//! 域 D23 `mall_sync` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::mall_sync` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    mall_sync::{
        CompleteMallSalesSyncJobRequest, CreateMallSalesReconciliationJobRequest,
        CreateMallSalesSyncJobRequest, CreateMasterMappingTaskRequest, IngestMallSalesOrderSnapshotsRequest,
        IngestMallSalesOrderSnapshotsResult, MallSalesOrderSnapshotListParams, MallSalesOrderSnapshotView,
        MallSalesReconciliationItemListParams, MallSalesReconciliationItemView,
        MallSalesReconciliationJobListParams, MallSalesReconciliationJobView, MallSalesSyncCursorView,
        MallSalesSyncJobListParams, MallSalesSyncJobView, MallSyncService, MasterMappingTaskListParams,
        MasterMappingTaskView, PageView, ResolveMallSalesReconciliationItemRequest,
        ResolveMasterMappingTaskRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询同步作业列表",
    resource = "mall_sales_sync_job",
    action = "list"
)]
/// 查询商城销售单同步作业列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn mall_sales_sync_job_list(
    State(state): State<AppState>,
    Query(params): Query<MallSalesSyncJobListParams>,
) -> Result<PageView<MallSalesSyncJobView>> {
    let page = MallSyncService::new(state.db()).sync_job_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "创建同步作业",
    resource = "mall_sales_sync_job",
    action = "create"
)]
/// 创建同步作业（来源商城经 D01 校验；同一商城只允许一个运行中的增量任务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建同步作业的响应视图。
pub async fn mall_sales_sync_job_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateMallSalesSyncJobRequest>,
) -> Result<MallSalesSyncJobView> {
    let view = MallSyncService::new(state.db())
        .create_sync_job(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询同步作业详情",
    resource = "mall_sales_sync_job",
    action = "detail"
)]
/// 查询同步作业详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 同步作业 ID
///
/// # 返回
/// 返回同步作业的响应视图。
pub async fn mall_sales_sync_job_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<MallSalesSyncJobView> {
    let view = MallSyncService::new(state.db()).sync_job_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "完成同步作业",
    resource = "mall_sales_sync_job",
    action = "complete"
)]
/// 完成同步作业（成功时前移同步水位；相同终态重复提交按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 同步作业 ID
/// * `req` - 完成请求（终态结果）
///
/// # 返回
/// 返回完成后的同步作业视图。
pub async fn mall_sales_sync_job_complete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CompleteMallSalesSyncJobRequest>,
) -> Result<MallSalesSyncJobView> {
    let view = MallSyncService::new(state.db())
        .complete_sync_job(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询商城销售单快照列表",
    resource = "mall_sales_order_snapshot",
    action = "list"
)]
/// 查询商城卡券销售单快照列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn mall_sales_order_snapshot_list(
    State(state): State<AppState>,
    Query(params): Query<MallSalesOrderSnapshotListParams>,
) -> Result<PageView<MallSalesOrderSnapshotView>> {
    let page = MallSyncService::new(state.db()).snapshot_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "落盘商城销售单快照",
    resource = "mall_sales_order_snapshot",
    action = "create"
)]
/// 落盘一页商城卡券销售单快照（事实键幂等：重复推送/迟到数据跳过，不产生重复快照）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 落盘请求（作业 + 本页快照）
///
/// # 返回
/// 返回本页落盘与跳过计数。
pub async fn mall_sales_order_snapshot_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<IngestMallSalesOrderSnapshotsRequest>,
) -> Result<IngestMallSalesOrderSnapshotsResult> {
    let result = MallSyncService::new(state.db())
        .ingest_snapshots(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询同步水位游标",
    resource = "mall_sales_sync_cursor",
    action = "detail"
)]
/// 查询同步水位游标（单来源商城单行；未建立时 `data` 为 `null`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - `source_system_id` 来源商城
///
/// # 返回
/// 返回水位游标视图或 `null`。
pub async fn mall_sales_sync_cursor_detail(
    State(state): State<AppState>,
    Query(query): Query<SourceSystemQuery>,
) -> Result<Option<MallSalesSyncCursorView>> {
    let view = MallSyncService::new(state.db())
        .sync_cursor_detail(&query.source_system_id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询核对作业列表",
    resource = "mall_sales_reconciliation_job",
    action = "list"
)]
/// 查询商城销售单核对作业列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn mall_sales_reconciliation_job_list(
    State(state): State<AppState>,
    Query(params): Query<MallSalesReconciliationJobListParams>,
) -> Result<PageView<MallSalesReconciliationJobView>> {
    let page = MallSyncService::new(state.db())
        .reconciliation_job_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "创建核对作业",
    resource = "mall_sales_reconciliation_job",
    action = "create"
)]
/// 创建核对作业并写入差异明细（原子；批次号重复按幂等返回既有作业）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建（或既有）核对作业的响应视图。
pub async fn mall_sales_reconciliation_job_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateMallSalesReconciliationJobRequest>,
) -> Result<MallSalesReconciliationJobView> {
    let view = MallSyncService::new(state.db())
        .create_reconciliation_job(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询核对差异明细列表",
    resource = "mall_sales_reconciliation_item",
    action = "list"
)]
/// 查询核对差异明细列表（按核对作业）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 核对作业 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn mall_sales_reconciliation_item_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<MallSalesReconciliationItemListParams>,
) -> Result<PageView<MallSalesReconciliationItemView>> {
    let page = MallSyncService::new(state.db())
        .reconciliation_item_list(&id, &params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "处理核对差异明细",
    resource = "mall_sales_reconciliation_item",
    action = "resolve"
)]
/// 处理核对差异明细（人工解决或确认无误；已终态重复提交按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 差异明细 ID
/// * `req` - 处理请求
///
/// # 返回
/// 返回处理后的差异明细视图。
pub async fn mall_sales_reconciliation_item_resolve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ResolveMallSalesReconciliationItemRequest>,
) -> Result<MallSalesReconciliationItemView> {
    let view = MallSyncService::new(state.db())
        .resolve_reconciliation_item(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询映射任务列表",
    resource = "master_mapping_task",
    action = "list"
)]
/// 查询商城快照基础资料映射任务列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn master_mapping_task_list(
    State(state): State<AppState>,
    Query(params): Query<MasterMappingTaskListParams>,
) -> Result<PageView<MasterMappingTaskView>> {
    let page = MallSyncService::new(state.db())
        .mapping_task_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "创建映射任务",
    resource = "master_mapping_task",
    action = "create"
)]
/// 创建商城快照基础资料映射任务（同一快照、映射类型只允许一个进行中任务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建映射任务的响应视图。
pub async fn master_mapping_task_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateMasterMappingTaskRequest>,
) -> Result<MasterMappingTaskView> {
    let view = MallSyncService::new(state.db())
        .create_mapping_task(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "处理映射任务",
    resource = "master_mapping_task",
    action = "resolve"
)]
/// 处理映射任务（已解决或无法处理；已终态重复提交按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 映射任务 ID
/// * `req` - 处理请求
///
/// # 返回
/// 返回处理后的映射任务视图。
pub async fn master_mapping_task_resolve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ResolveMasterMappingTaskRequest>,
) -> Result<MasterMappingTaskView> {
    let view = MallSyncService::new(state.db())
        .resolve_mapping_task(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 水位游标查询参数（`source_system_id` 来源商城）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SourceSystemQuery {
    /// 来源商城。
    pub source_system_id: String,
}
