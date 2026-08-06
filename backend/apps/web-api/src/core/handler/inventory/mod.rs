//! 域 D17 `inventory` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::inventory` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    inventory::{
        ApproveStockAdjustmentRequest, CreateStockAdjustmentRequest, InventoryService, PageView,
        StockAdjustmentDetailView, StockAdjustmentListParams, StockAdjustmentView, StockBalanceDetailView,
        StockBalanceListParams, StockBalanceView, StockMovementListParams, StockMovementView,
        StockReservationListParams, StockReservationView, SubmitStockAdjustmentRequest,
        UpdateStockAdjustmentRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存余额列表",
    resource = "stock_balance",
    action = "list"
)]
/// 查询库存余额列表（W10 台账）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`warehouse_id`/`sku_id` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn stock_balance_list(
    State(state): State<AppState>,
    Query(params): Query<StockBalanceListParams>,
) -> Result<PageView<StockBalanceView>> {
    let page = InventoryService::new(state.db())
        .stock_balance_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存余额详情",
    resource = "stock_balance",
    action = "detail"
)]
/// 查询库存余额详情（余额 + 最近流水 + 有效预占 + 未过账调整）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 余额主键
///
/// # 返回
/// 返回余额详情视图。
pub async fn stock_balance_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StockBalanceDetailView> {
    let view = InventoryService::new(state.db())
        .stock_balance_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存流水台账",
    resource = "stock_movement",
    action = "list"
)]
/// 查询库存流水台账（W10 流水视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（仓库/SKU/类型/方向/时间区间）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn stock_movement_list(
    State(state): State<AppState>,
    Query(params): Query<StockMovementListParams>,
) -> Result<PageView<StockMovementView>> {
    let page = InventoryService::new(state.db())
        .stock_movement_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存预占列表",
    resource = "stock_reservation",
    action = "list"
)]
/// 查询库存预占列表（W10 销售预占视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（仓库/SKU/状态/销售明细）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn stock_reservation_list(
    State(state): State<AppState>,
    Query(params): Query<StockReservationListParams>,
) -> Result<PageView<StockReservationView>> {
    let page = InventoryService::new(state.db())
        .stock_reservation_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存调整单列表",
    resource = "stock_adjustment",
    action = "list"
)]
/// 查询库存调整单列表（W10 调整记录视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（仓库/状态）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn stock_adjustment_list(
    State(state): State<AppState>,
    Query(params): Query<StockAdjustmentListParams>,
) -> Result<PageView<StockAdjustmentView>> {
    let page = InventoryService::new(state.db())
        .stock_adjustment_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存调整单详情",
    resource = "stock_adjustment",
    action = "detail"
)]
/// 查询库存调整单详情（表头 + 明细 + 过账流水）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 调整单主键
///
/// # 返回
/// 返回调整单详情视图。
pub async fn stock_adjustment_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StockAdjustmentDetailView> {
    let view = InventoryService::new(state.db())
        .stock_adjustment_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "创建库存调整单",
    resource = "stock_adjustment",
    action = "create"
)]
/// 创建库存调整单（草稿）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 明细）
///
/// # 返回
/// 返回新建调整单的响应视图。
pub async fn stock_adjustment_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateStockAdjustmentRequest>,
) -> Result<StockAdjustmentView> {
    let view = InventoryService::new(state.db())
        .create_stock_adjustment(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "更新库存调整单",
    resource = "stock_adjustment",
    action = "update"
)]
/// 更新库存调整单（仅草稿/驳回；乐观锁冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 调整单主键
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后调整单的响应视图。
pub async fn stock_adjustment_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateStockAdjustmentRequest>,
) -> Result<StockAdjustmentView> {
    let view = InventoryService::new(state.db())
        .update_stock_adjustment(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "提交库存调整仓储复核",
    resource = "stock_adjustment",
    action = "submit"
)]
/// 提交库存调整仓储复核（草稿/驳回 → 待仓储复核）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 调整单主键
/// * `req` - 提交请求（仓储复核人）
///
/// # 返回
/// 返回提交后的调整单视图。
pub async fn stock_adjustment_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitStockAdjustmentRequest>,
) -> Result<StockAdjustmentView> {
    let view = InventoryService::new(state.db())
        .submit_stock_adjustment(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "库存调整仓储复核通过",
    resource = "stock_adjustment",
    action = "approve"
)]
/// 库存调整仓储复核通过，提交财务确认（待仓储复核 → 待财务确认）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 调整单主键
/// * `req` - 确认请求（成本影响确认人）
///
/// # 返回
/// 返回提交财务确认后的调整单视图。
pub async fn stock_adjustment_approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ApproveStockAdjustmentRequest>,
) -> Result<StockAdjustmentView> {
    let view = InventoryService::new(state.db())
        .approve_stock_adjustment(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "驳回库存调整",
    resource = "stock_adjustment",
    action = "reject"
)]
/// 驳回库存调整（待仓储复核/待财务确认 → 驳回）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 调整单主键
///
/// # 返回
/// 返回驳回后的调整单视图。
pub async fn stock_adjustment_reject(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<StockAdjustmentView> {
    let view = InventoryService::new(state.db())
        .reject_stock_adjustment(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "过账库存调整",
    resource = "stock_adjustment",
    action = "post"
)]
/// 过账库存调整（待财务确认 → 已过账；流水+余额+适用预占同事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 调整单主键
///
/// # 返回
/// 返回过账后的调整单视图。
pub async fn stock_adjustment_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<StockAdjustmentView> {
    let view = InventoryService::new(state.db())
        .post_stock_adjustment(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
