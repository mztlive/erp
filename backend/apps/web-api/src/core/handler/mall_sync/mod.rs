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
        CompleteMallSalesSyncJobRequest, ConfirmMappingCommand, ConfirmMappingResult,
        CreateMallSalesReconciliationJobRequest, CreateMasterMappingTaskRequest, GovernanceActionResult,
        IngestMallSalesOrderSnapshotsRequest, IngestMallSalesOrderSnapshotsResult,
        MallSalesOrderSnapshotListParams, MallSalesOrderSnapshotView, MallSalesReconciliationItemListParams,
        MallSalesReconciliationItemView, MallSalesReconciliationJobListParams,
        MallSalesReconciliationJobView, MallSalesSyncCursorView, MallSalesSyncJobListParams,
        MallSalesSyncJobView, MallSyncService, MasterMappingTaskDetailParams, MasterMappingTaskListParams,
        MasterMappingTaskView, PageView, ReapplyMallSnapshotCommand, ReapplyOperationView,
        RequestSourceFixCommand, RequestSourceFixResult, ResolveMallSalesReconciliationItemRequest,
        RetryMallSalesSyncJobRequest, TriggerMallSyncCommand,
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
/// 按阶段强判别命令触发同步作业。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `command` - 人工、调度、重试或核对触发命令
///
/// # 返回
/// 返回新建同步作业的响应视图。
pub async fn mall_sales_sync_job_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(command): Json<TriggerMallSyncCommand>,
) -> Result<MallSalesSyncJobView> {
    let view = MallSyncService::new(state.db())
        .trigger_sync_job(command, &actor)
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
    desc = "原子重试失败同步作业",
    resource = "mall_sales_sync_job",
    action = "create"
)]
/// 沿服务端保存的失败作业事实创建重试作业。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待重试作业 ID
/// * `request` - 重试理由与幂等信息
///
/// # 返回
/// 返回原子创建的重试作业视图。
pub async fn mall_sales_sync_job_retry(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<RetryMallSalesSyncJobRequest>,
) -> Result<MallSalesSyncJobView> {
    let view = MallSyncService::new(state.db())
        .retry_sync_job(&id, request, &actor)
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
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<MasterMappingTaskListParams>,
) -> Result<PageView<MasterMappingTaskView>> {
    let page = MallSyncService::new(state.db())
        .mapping_task_list(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询映射任务详情",
    resource = "master_mapping_task",
    action = "detail"
)]
/// 查询当前 actor 的映射任务、正式责任、候选、谱系和领域动作投影。
pub async fn master_mapping_task_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<MasterMappingTaskDetailParams>,
) -> Result<MasterMappingTaskView> {
    let view = MallSyncService::new(state.db())
        .mapping_task_detail(&id, &params, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
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
    desc = "确认映射目标",
    resource = "master_mapping_task",
    action = "confirm"
)]
/// 按固定映射注册表确认规范目标，并原子完成谱系、映射任务和正式任务。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 映射任务 ID
/// * `req` - 处理请求
///
/// # 返回
/// 返回处理后的映射任务视图。
pub async fn master_mapping_task_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ConfirmMappingCommand>,
) -> Result<ConfirmMappingResult> {
    let result = MallSyncService::new(state.db())
        .confirm_mapping_task(&id, command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "请求来源系统修复映射证据",
    resource = "master_mapping_task",
    action = "request_source_fix"
)]
/// 追加来源修复证据并保持正式任务开放。
///
/// # 返回
/// 返回新增证据标识与当前正式任务版本。
pub async fn master_mapping_task_request_source_fix(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<RequestSourceFixCommand>,
) -> Result<RequestSourceFixResult> {
    let result = MallSyncService::new(state.db())
        .request_mapping_source_fix(&id, command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "重新归集商城快照",
    resource = "master_mapping_task",
    action = "reapply"
)]
/// 创建或幂等返回独立重新归集操作。
///
/// # 返回
/// 返回可按 operation ID 继续查询的正式操作结果。
pub async fn master_mapping_task_reapply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ReapplyMallSnapshotCommand>,
) -> Result<GovernanceActionResult> {
    let result = MallSyncService::new(state.db())
        .reapply_mapping_task(&id, command, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "商城同步与映射",
    group_desc = "商城卡券销售单同步作业、快照、核对与映射任务管理",
    desc = "查询重新归集操作结果",
    resource = "master_mapping_task",
    action = "detail"
)]
/// 按独立 operation ID 查询重新归集状态与正式结果。
pub async fn master_mapping_task_reapply_operation_detail(
    State(state): State<AppState>,
    Path((id, operation_id)): Path<(String, String)>,
) -> Result<ReapplyOperationView> {
    let result = MallSyncService::new(state.db())
        .reapply_operation_detail(&id, &operation_id)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 水位游标查询参数（`source_system_id` 来源商城）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SourceSystemQuery {
    /// 来源商城。
    pub source_system_id: String,
}
