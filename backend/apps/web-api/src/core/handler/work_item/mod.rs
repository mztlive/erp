//! 域 D03 `work_item` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::work_item` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    work_item::{
        ClaimWorkItemRequest, CloseWorkItemRequest, CompleteWorkItemRequest, DeferWorkItemRequest,
        DispatchWorkItemRequest, PageView, TransferWorkItemRequest, WorkItemListParams, WorkItemService,
        WorkItemView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "查询待办列表",
    resource = "work_item",
    action = "list"
)]
/// 查询待办列表（工作队列）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`owner_role`/`owner_user_id`/`status` 等）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn work_item_list(
    State(state): State<AppState>,
    Query(params): Query<WorkItemListParams>,
) -> Result<PageView<WorkItemView>> {
    let page = WorkItemService::new(state.db()).work_item_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "查询待办详情",
    resource = "work_item",
    action = "detail"
)]
/// 查询待办详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 待办 ID
///
/// # 返回
/// 返回完整待办视图。
pub async fn work_item_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db()).work_item_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "派发待办",
    resource = "work_item",
    action = "create"
)]
/// 派发正式待办。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 派发请求（`{ work_item_type, business_object_type, ... }`）
///
/// # 返回
/// 返回新建的待办视图。
pub async fn work_item_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<DispatchWorkItemRequest>,
) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db())
        .dispatch_work_item(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "领取待办",
    resource = "work_item",
    action = "claim"
)]
/// 领取待办（条件更新原子完成，行锁语义）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待办 ID
/// * `req` - 领取请求（含期望版本）
///
/// # 返回
/// 返回领取后的待办视图。
pub async fn work_item_claim(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ClaimWorkItemRequest>,
) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db())
        .claim_work_item(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "暂挂待办",
    resource = "work_item",
    action = "defer"
)]
/// 暂挂待办（任务回到待领取状态并清除责任人）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待办 ID
/// * `req` - 暂挂请求（含期望版本）
///
/// # 返回
/// 返回暂挂后的待办视图。
pub async fn work_item_defer(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DeferWorkItemRequest>,
) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db())
        .defer_work_item(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "转交待办",
    resource = "work_item",
    action = "transfer"
)]
/// 转交待办（更新责任角色与责任人，任务保持处理中）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待办 ID
/// * `req` - 转交请求（含期望版本与新责任人）
///
/// # 返回
/// 返回转交后的待办视图。
pub async fn work_item_transfer(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<TransferWorkItemRequest>,
) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db())
        .transfer_work_item(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "完成任务",
    resource = "work_item",
    action = "complete"
)]
/// 正式完成任务（业务事实由对应强类型事务完成，本接口只终结任务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待办 ID
/// * `req` - 完成请求（含期望版本）
///
/// # 返回
/// 返回完成后的待办视图。
pub async fn work_item_complete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CompleteWorkItemRequest>,
) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db())
        .complete_work_item(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "统一待办",
    group_desc = "正式待办队列与处理",
    desc = "关闭待办",
    resource = "work_item",
    action = "close"
)]
/// 关闭误派/重复待办（必须记录结构化原因；审批/确认/结果未知类不可人工关闭）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 待办 ID
/// * `req` - 关闭请求（含期望版本与关闭原因）
///
/// # 返回
/// 返回关闭后的待办视图。
pub async fn work_item_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CloseWorkItemRequest>,
) -> Result<WorkItemView> {
    let view = WorkItemService::new(state.db())
        .close_work_item(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
