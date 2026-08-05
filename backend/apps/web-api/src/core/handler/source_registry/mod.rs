//! 域 D01 `source_registry` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::source_registry` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    source_registry::{
        CreateExternalIdentityMapRequest, CreateSourceSystemRequest, ExternalIdentityMapListParams,
        ExternalIdentityMapView, PageView, SourceRegistryService, SourceSystemListParams, SourceSystemView,
        UpdateSourceSystemRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "来源注册",
    group_desc = "来源系统与外部身份映射管理",
    desc = "查询来源系统列表",
    resource = "source_system",
    action = "list"
)]
/// 查询来源系统列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn source_system_list(
    State(state): State<AppState>,
    Query(params): Query<SourceSystemListParams>,
) -> Result<PageView<SourceSystemView>> {
    let page = SourceRegistryService::new(state.db())
        .source_system_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "来源注册",
    group_desc = "来源系统与外部身份映射管理",
    desc = "创建来源系统",
    resource = "source_system",
    action = "create"
)]
/// 创建来源系统。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ code, name, system_type }`）
///
/// # 返回
/// 返回新建来源系统的响应视图。
pub async fn source_system_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSourceSystemRequest>,
) -> Result<SourceSystemView> {
    let view = SourceRegistryService::new(state.db())
        .create_source_system(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "来源注册",
    group_desc = "来源系统与外部身份映射管理",
    desc = "更新来源系统",
    resource = "source_system",
    action = "update"
)]
/// 更新来源系统（乐观锁：请求携带期望版本，冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 来源系统 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后来源系统的响应视图。
pub async fn source_system_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSourceSystemRequest>,
) -> Result<SourceSystemView> {
    let view = SourceRegistryService::new(state.db())
        .update_source_system(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "来源注册",
    group_desc = "来源系统与外部身份映射管理",
    desc = "查询外部身份映射列表",
    resource = "external_identity_map",
    action = "list"
)]
/// 查询外部身份映射列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`source_system_id` 等扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn external_identity_map_list(
    State(state): State<AppState>,
    Query(params): Query<ExternalIdentityMapListParams>,
) -> Result<PageView<ExternalIdentityMapView>> {
    let page = SourceRegistryService::new(state.db())
        .external_identity_map_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "来源注册",
    group_desc = "来源系统与外部身份映射管理",
    desc = "建立外部身份映射",
    resource = "external_identity_map",
    action = "create"
)]
/// 建立外部身份映射（跨集合事务：映射身份 + 目标谱系原子写入）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建映射的响应视图。
pub async fn external_identity_map_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateExternalIdentityMapRequest>,
) -> Result<ExternalIdentityMapView> {
    let view = SourceRegistryService::new(state.db())
        .create_external_identity_map(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
