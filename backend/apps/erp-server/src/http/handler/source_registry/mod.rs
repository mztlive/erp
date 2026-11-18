//! 域 D01 `source_registry` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 支撑领域的 DTO，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_supplier_catalog::source_registry::create_source_system;
use erp_support::{
    CreateExternalIdentityMapRequest, CreateSourceSystemRequest, ExternalIdentityMapListParams,
    ExternalIdentityMapView, PageView, SourceSystemListParams, SourceSystemView, UpdateSourceSystemRequest,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 按请求条件查询来源系统列表。
pub async fn source_system_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SourceSystemListParams>,
) -> Result<PageView<SourceSystemView>> {
    let page = state.application().source_registry_service().source_system_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建来源系统。
/// `req` 为创建请求（`{ code, name, system_type }`）。
/// 返回新建来源系统的响应视图。
pub async fn source_system_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSourceSystemRequest>,
) -> Result<SourceSystemView> {
    let view =
        create_source_system(state.application().db(), state.application().authorization(), req, actor)
            .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 更新来源系统（乐观锁：请求携带期望版本，冲突返回 409）。
/// `id` 为来源系统 ID，`req` 为更新请求（含期望版本）。
/// 返回更新后来源系统的响应视图。
pub async fn source_system_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSourceSystemRequest>,
) -> Result<SourceSystemView> {
    let view = state.application().source_registry_service().update_source_system(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按请求条件查询外部身份映射列表。
/// `query` 为分页与筛选参数（`source_system_id` 等扁平传递）。
pub async fn external_identity_map_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ExternalIdentityMapListParams>,
) -> Result<PageView<ExternalIdentityMapView>> {
    let page =
        state.application().source_registry_service().external_identity_map_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 建立外部身份映射（跨集合事务：映射身份 + 目标谱系原子写入）。
/// 返回新建映射的响应视图。
pub async fn external_identity_map_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateExternalIdentityMapRequest>,
) -> Result<ExternalIdentityMapView> {
    let view =
        state.application().source_registry_service().create_external_identity_map(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}
