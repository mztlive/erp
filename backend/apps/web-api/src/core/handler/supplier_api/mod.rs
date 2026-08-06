//! 域 D25 `supplier_api` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::supplier_api` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//! 外部调用网关（`SupplierApiGateway`）在 handler 内构造默认失败关闭实现，
//! 保持 Service 可注入 mock 网关。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    supplier_api::{
        CreateSupplierApiConnectionRequest, HealthCheckRequest, HealthCheckView, PageView,
        ReplaceCapabilitiesRequest, SupplierApiCapabilityListParams, SupplierApiCapabilityView,
        SupplierApiConnectionDetailView, SupplierApiConnectionListParams, SupplierApiConnectionView,
        SupplierApiService, UnavailableSupplierApiGateway, UpdateSupplierApiConnectionRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询供应商 API 连接列表",
    resource = "supplier_api_connection",
    action = "list"
)]
/// 查询供应商 API 连接列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_api_connection_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierApiConnectionListParams>,
) -> Result<PageView<SupplierApiConnectionView>> {
    let page = SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway))
        .connection_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询供应商 API 连接详情",
    resource = "supplier_api_connection",
    action = "detail"
)]
/// 查询供应商 API 连接详情（连接身份 + 能力清单）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 连接 ID
///
/// # 返回
/// 返回连接详情视图。
pub async fn supplier_api_connection_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierApiConnectionDetailView> {
    let view = SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway))
        .connection_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "创建供应商 API 连接",
    resource = "supplier_api_connection",
    action = "create"
)]
/// 创建供应商 API 连接及其能力声明（跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（连接身份 + 能力清单）
///
/// # 返回
/// 返回新建连接的响应视图。
pub async fn supplier_api_connection_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierApiConnectionRequest>,
) -> Result<SupplierApiConnectionView> {
    let view = SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway))
        .create_connection(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "更新供应商 API 连接",
    resource = "supplier_api_connection",
    action = "update"
)]
/// 更新供应商 API 连接（乐观锁：请求携带期望版本，冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 连接 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后连接的响应视图。
pub async fn supplier_api_connection_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSupplierApiConnectionRequest>,
) -> Result<SupplierApiConnectionView> {
    let view = SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway))
        .update_connection(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "替换供应商 API 连接能力清单",
    resource = "supplier_api_connection",
    action = "update"
)]
/// 原子替换连接能力声明（期望连接版本冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 连接 ID
/// * `req` - 替换请求（含期望连接版本）
///
/// # 返回
/// 返回替换后的能力清单。
pub async fn supplier_api_capabilities_replace(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ReplaceCapabilitiesRequest>,
) -> Result<Vec<SupplierApiCapabilityView>> {
    let view = SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway))
        .replace_capabilities(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询连接能力列表",
    resource = "supplier_api_capability",
    action = "list"
)]
/// 查询连接能力声明列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`connection_id` 等扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_api_capability_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierApiCapabilityListParams>,
) -> Result<PageView<SupplierApiCapabilityView>> {
    let page = SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway))
        .capability_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "执行连接健康检查",
    resource = "supplier_api_connection",
    action = "health_check"
)]
/// 执行连接健康检查（外部调用在事务之外，结果经 `inbox_message` +
/// `integration_error_task` 承接）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 连接 ID
/// * `req` - 健康检查请求（含幂等键）
///
/// # 返回
/// 返回健康检查结果视图（含消息信封与错误任务 ID）。
pub async fn supplier_api_connection_health_check(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<HealthCheckRequest>,
) -> Result<HealthCheckView> {
    let view = SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway))
        .run_health_check(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
