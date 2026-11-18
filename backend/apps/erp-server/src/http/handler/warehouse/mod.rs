//! 域 D11 `warehouse` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_warehouse` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use application_core::directory::ScopedPage;
use axum::Extension;
use axum::extract::{Path, State};
use erp_warehouse::{
    CreateWarehouseRequest, CreateWarehouseSkuPolicyRequest, PageView,
    UpdateWarehouseFulfillmentHandlersRequest, UpdateWarehouseRequest, UpdateWarehouseSkuPolicyRequest,
    WarehouseFulfillmentHandlerOptionView, WarehouseListParams, WarehouseRevisionListParams,
    WarehouseRevisionView, WarehouseSkuPolicyListParams, WarehouseSkuPolicyView, WarehouseView,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::warehouse_directory;

/// 按请求条件查询仓库列表。
/// `query` 为分页与筛选参数（`warehouse_code`/`status` 扁平传递）。
pub async fn warehouse_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WarehouseListParams>,
) -> Result<ScopedPage<WarehouseView>> {
    let page = warehouse_directory(state.application().db(), state.application().authorization())
        .warehouses(actor, params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建仓库（仓库稳定身份 + 首个修订，跨集合事务）。
/// `req` 为创建请求（`{ warehouse_code, name, address, contact, effective_from, ... }`）。
/// 返回新建仓库的响应视图。
pub async fn warehouse_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateWarehouseRequest>,
) -> Result<WarehouseView> {
    let view = state.application().warehouse_service().warehouse_create(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 更新仓库（追加新修订并更新稳定身份，跨集合事务；乐观锁）。
/// `req` 为更新请求（含期望版本与新修订快照）。
/// 返回更新后仓库的响应视图。
pub async fn warehouse_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWarehouseRequest>,
) -> Result<WarehouseView> {
    let view = state.application().warehouse_service().warehouse_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 更新仓库入库与仓发经办人。
/// `req` 为期望版本与两个具体账号。
/// 返回更新后的仓库视图。
pub async fn warehouse_fulfillment_handlers_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWarehouseFulfillmentHandlersRequest>,
) -> Result<WarehouseView> {
    let view = state
        .application()
        .warehouse_service()
        .warehouse_fulfillment_handlers_update(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询可用的仓库收发经办人选项。
/// 返回可登录账号及其入库、仓发资格。
pub async fn warehouse_fulfillment_handler_options(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<Vec<WarehouseFulfillmentHandlerOptionView>> {
    let options =
        state.application().warehouse_service().warehouse_fulfillment_handler_options(&actor).await?;
    Ok(ApiResponse::ok_with_data(options))
}

/// 查询仓库修订列表（不含加密地址/联系人等敏感字段）。
/// `query` 为分页与筛选参数（`warehouse_id`/`name` 扁平传递）。
pub async fn warehouse_revision_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WarehouseRevisionListParams>,
) -> Result<PageView<WarehouseRevisionView>> {
    let page = state.application().warehouse_service().warehouse_revision_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 按请求条件查询仓库-SKU 预警策略列表。
/// `query` 为分页与筛选参数（`warehouse_id`/`sku_id`/`status` 扁平传递）。
pub async fn warehouse_sku_policy_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<WarehouseSkuPolicyListParams>,
) -> Result<PageView<WarehouseSkuPolicyView>> {
    let page = state.application().warehouse_service().warehouse_sku_policy_list(&actor, &params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建仓库-SKU 预警策略（启用区间不得与既有策略重叠）。
/// `req` 为创建请求（`{ warehouse_id, sku_id, minimum_available_quantity, ... }`）。
/// 返回新建策略的响应视图。
pub async fn warehouse_sku_policy_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateWarehouseSkuPolicyRequest>,
) -> Result<WarehouseSkuPolicyView> {
    let view = erp_master_data::create_warehouse_sku_policy(
        state.application().db(),
        state.application().authorization(),
        req,
        actor,
    )
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 更新仓库-SKU 预警策略（乐观锁）。
/// `req` 为更新请求（含期望版本）。
/// 返回更新后策略的响应视图。
pub async fn warehouse_sku_policy_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWarehouseSkuPolicyRequest>,
) -> Result<WarehouseSkuPolicyView> {
    let view = state.application().warehouse_service().warehouse_sku_policy_update(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 删除仓库-SKU 预警策略（软删除）。
/// 返回删除结果（`data` 为 `null`）。
pub async fn warehouse_sku_policy_delete(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().warehouse_service().warehouse_sku_policy_delete(&id, &actor).await?;

    Ok(ApiResponse::ok())
}

pub mod directory;
