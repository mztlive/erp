//! 域 D11 `warehouse` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::warehouse` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::audit::AuditActor;
use services::warehouse::{
    CreateWarehouseRequest, CreateWarehouseSkuPolicyRequest, PageView, UpdateWarehouseRequest,
    UpdateWarehouseSkuPolicyRequest, WarehouseListParams, WarehouseRevisionListParams, WarehouseRevisionView,
    WarehouseService, WarehouseSkuPolicyListParams, WarehouseSkuPolicyView, WarehouseView,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询仓库列表",
    resource = "warehouse",
    action = "list"
)]
/// 查询仓库列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`warehouse_code`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn warehouse_list(
    State(state): State<AppState>,
    Query(params): Query<WarehouseListParams>,
) -> Result<PageView<WarehouseView>> {
    let page = WarehouseService::new(state.db()).warehouse_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建仓库",
    resource = "warehouse",
    action = "create"
)]
/// 创建仓库（仓库稳定身份 + 首个修订，跨集合事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ warehouse_code, name, address, contact, effective_from, ... }`）
///
/// # 返回
/// 返回新建仓库的响应视图。
pub async fn warehouse_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateWarehouseRequest>,
) -> Result<WarehouseView> {
    let view = WarehouseService::new(state.db())
        .warehouse_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新仓库",
    resource = "warehouse",
    action = "update"
)]
/// 更新仓库（追加新修订并更新稳定身份，跨集合事务；乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 仓库 ID
/// * `req` - 更新请求（含期望版本与新修订快照）
///
/// # 返回
/// 返回更新后仓库的响应视图。
pub async fn warehouse_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWarehouseRequest>,
) -> Result<WarehouseView> {
    let view = WarehouseService::new(state.db())
        .warehouse_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询仓库修订列表",
    resource = "warehouse_revision",
    action = "list"
)]
/// 查询仓库修订列表（不含加密地址/联系人等敏感字段）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`warehouse_id`/`name` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn warehouse_revision_list(
    State(state): State<AppState>,
    Query(params): Query<WarehouseRevisionListParams>,
) -> Result<PageView<WarehouseRevisionView>> {
    let page = WarehouseService::new(state.db())
        .warehouse_revision_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询仓库SKU预警策略列表",
    resource = "warehouse_sku_policy",
    action = "list"
)]
/// 查询仓库-SKU 预警策略列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`warehouse_id`/`sku_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn warehouse_sku_policy_list(
    State(state): State<AppState>,
    Query(params): Query<WarehouseSkuPolicyListParams>,
) -> Result<PageView<WarehouseSkuPolicyView>> {
    let page = WarehouseService::new(state.db())
        .warehouse_sku_policy_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "创建仓库SKU预警策略",
    resource = "warehouse_sku_policy",
    action = "create"
)]
/// 创建仓库-SKU 预警策略（启用区间不得与既有策略重叠）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`{ warehouse_id, sku_id, minimum_available_quantity, ... }`）
///
/// # 返回
/// 返回新建策略的响应视图。
pub async fn warehouse_sku_policy_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateWarehouseSkuPolicyRequest>,
) -> Result<WarehouseSkuPolicyView> {
    let view = WarehouseService::new(state.db())
        .warehouse_sku_policy_create(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "更新仓库SKU预警策略",
    resource = "warehouse_sku_policy",
    action = "update"
)]
/// 更新仓库-SKU 预警策略（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 策略 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后策略的响应视图。
pub async fn warehouse_sku_policy_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWarehouseSkuPolicyRequest>,
) -> Result<WarehouseSkuPolicyView> {
    let view = WarehouseService::new(state.db())
        .warehouse_sku_policy_update(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "删除仓库SKU预警策略",
    resource = "warehouse_sku_policy",
    action = "delete"
)]
/// 删除仓库-SKU 预警策略（软删除）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 策略 ID
///
/// # 返回
/// 返回删除结果（`data` 为 `null`）。
pub async fn warehouse_sku_policy_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    WarehouseService::new(state.db())
        .warehouse_sku_policy_delete(&id, &actor)
        .await?;

    Ok(ApiResponse::ok())
}
