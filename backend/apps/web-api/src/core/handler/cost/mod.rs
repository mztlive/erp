//! 域 D20 `cost` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::cost` 的 DTO。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    cost::{
        CostAllocationListParams, CostAllocationView, CostEntryListParams, CostEntryView, CostService,
        CreateCostEntryRequest, PageView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本事实列表",
    resource = "cost_entry",
    action = "list"
)]
/// 查询成本事实列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn cost_entry_list(
    State(state): State<AppState>,
    Query(params): Query<CostEntryListParams>,
) -> Result<PageView<CostEntryView>> {
    let page = CostService::new(state.db()).cost_entry_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本事实详情",
    resource = "cost_entry",
    action = "detail"
)]
/// 查询成本事实详情（事实 + 分配行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 成本事实 ID
///
/// # 返回
/// 返回完整成本视图。
pub async fn cost_entry_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<CostEntryView> {
    let view = CostService::new(state.db()).cost_entry_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "手工登记成本事实",
    resource = "cost_entry",
    action = "create"
)]
/// 手工登记成本事实与分配行（跨集合事务：事实 + 分配行原子可见）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建成本事实视图。
pub async fn cost_entry_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCostEntryRequest>,
) -> Result<CostEntryView> {
    let view = CostService::new(state.db())
        .create_cost_entry(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询成本分配列表",
    resource = "cost_allocation",
    action = "list"
)]
/// 查询成本分配列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`cost_entry_id`/`sales_order_id`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn cost_allocation_list(
    State(state): State<AppState>,
    Query(params): Query<CostAllocationListParams>,
) -> Result<PageView<CostAllocationView>> {
    let page = CostService::new(state.db()).cost_allocation_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}
