//! 域 D22 `legacy_import` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::legacy_import` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    legacy_import::{
        ApplyLegacyImportBatchRequest, CreateLegacyImportBatchRequest, CreateLegacyImportConfirmationRequest,
        DecideLegacyImportConfirmationRequest, LegacyImportBatchListItem, LegacyImportBatchListParams,
        LegacyImportBatchView, LegacyImportConfirmationListParams, LegacyImportConfirmationView,
        LegacyImportRowListParams, LegacyImportRowView, LegacyImportService, PageView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "查询导入批次列表",
    resource = "legacy_import_batch",
    action = "list"
)]
/// 查询导入批次列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn legacy_import_batch_list(
    State(state): State<AppState>,
    Query(params): Query<LegacyImportBatchListParams>,
) -> Result<PageView<LegacyImportBatchListItem>> {
    let page = LegacyImportService::new(state.db()).batch_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "创建导入批次",
    resource = "legacy_import_batch",
    action = "create"
)]
/// 创建导入批次（批次 + 来源行 + 后台任务原子写入，批次号重复按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（批次头 + 来源行）
///
/// # 返回
/// 返回新建（或既有）批次的响应视图。
pub async fn legacy_import_batch_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateLegacyImportBatchRequest>,
) -> Result<LegacyImportBatchView> {
    let view = LegacyImportService::new(state.db())
        .create_batch(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "查询导入批次详情",
    resource = "legacy_import_batch",
    action = "detail"
)]
/// 查询导入批次详情（含后台任务关联）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 导入批次 ID
///
/// # 返回
/// 返回批次的响应视图。
pub async fn legacy_import_batch_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<LegacyImportBatchView> {
    let view = LegacyImportService::new(state.db()).batch_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "应用导入批次",
    resource = "legacy_import_batch",
    action = "apply"
)]
/// 应用导入批次（后台应用阶段逐行结果，批次/行终态后重复提交按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 导入批次 ID
/// * `req` - 逐行结果
///
/// # 返回
/// 返回应用后批次的响应视图。
pub async fn legacy_import_batch_apply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ApplyLegacyImportBatchRequest>,
) -> Result<LegacyImportBatchView> {
    let view = LegacyImportService::new(state.db())
        .apply_batch(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "查询导入行列表",
    resource = "legacy_import_row",
    action = "list"
)]
/// 查询导入行列表（按批次）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 导入批次 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn legacy_import_row_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<LegacyImportRowListParams>,
) -> Result<PageView<LegacyImportRowView>> {
    let page = LegacyImportService::new(state.db())
        .row_list(&id, &params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "查询导入确认事实列表",
    resource = "legacy_import_confirmation",
    action = "list"
)]
/// 查询导入确认事实列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`batch_id` 为主要筛选）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn legacy_import_confirmation_list(
    State(state): State<AppState>,
    Query(params): Query<LegacyImportConfirmationListParams>,
) -> Result<PageView<LegacyImportConfirmationView>> {
    let page = LegacyImportService::new(state.db())
        .confirmation_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "创建导入确认事实",
    resource = "legacy_import_confirmation",
    action = "create"
)]
/// 创建待确认确认事实（批次推进到待确认阶段，重复提交按幂等返回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建（或既有）确认事实的响应视图。
pub async fn legacy_import_confirmation_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateLegacyImportConfirmationRequest>,
) -> Result<LegacyImportConfirmationView> {
    let view = LegacyImportService::new(state.db())
        .create_confirmation(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "导入与期初",
    group_desc = "旧数据导入批次、导入行与业务确认事实管理",
    desc = "决策导入确认事实",
    resource = "legacy_import_confirmation",
    action = "decide"
)]
/// 决策导入确认事实（`CONFIRM_SCOPE` 或 `RETURN_FOR_FIX`；全绿时批次推进到导入中）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 确认事实 ID
/// * `req` - 决策请求
///
/// # 返回
/// 返回决策后确认事实的响应视图。
pub async fn legacy_import_confirmation_decide(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DecideLegacyImportConfirmationRequest>,
) -> Result<LegacyImportConfirmationView> {
    let view = LegacyImportService::new(state.db())
        .decide_confirmation(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
