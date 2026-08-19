//! 域 D14 `sales_review` 的 HTTP handler。
//!
//! 仅保留销售变更单生命周期。卡券专用审批、采购确认与低毛利入口已删除。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    sales_review::{
        CancelSalesChangeApprovalRequest, CreateSalesChangeOrderRequest, PageView,
        SalesChangeOrderDetailView, SalesChangeOrderListParams, SalesChangeOrderView, SalesReviewService,
        SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "查询销售变更单列表",
    resource = "sales_change_order",
    action = "list"
)]
/// 查询销售变更单列表（W05 变更轨）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn sales_change_order_list(
    State(state): State<AppState>,
    Query(params): Query<SalesChangeOrderListParams>,
) -> Result<PageView<SalesChangeOrderView>> {
    let page = SalesReviewService::new(state.db())
        .sales_change_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "查询销售变更单详情",
    resource = "sales_change_order",
    action = "detail"
)]
/// 查询销售变更单详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 变更单 ID
///
/// # 返回
/// 返回详情视图。
pub async fn sales_change_order_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .sales_change_order_detail(&id)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "创建销售变更单",
    resource = "sales_change_order",
    action = "create"
)]
/// 创建销售变更单（草稿 + 变更工作副本原子形成）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（含变更类型、原因与目标草稿）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesChangeOrderRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let rbac = state.rbac();
    let view = SalesReviewService::new(state.db())
        .create_sales_change_order(req, &actor, &rbac)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "发起销售变更影响确认",
    resource = "sales_change_order",
    action = "submit"
)]
/// 提交销售变更并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 提交请求（含期望版本与幂等键）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_submit_impact(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSalesChangeRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .submit_sales_change(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "作废销售变更单",
    resource = "sales_change_order",
    action = "delete"
)]
/// 作废销售变更单（仅草稿态，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 作废请求（含期望版本）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_void(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidSalesChangeOrderRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .void_sales_change(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售复核",
    group_desc = "销售审批与采购二次确认（W05/W07）管理",
    desc = "撤回销售变更审批",
    resource = "sales_change_order",
    action = "submit"
)]
/// 撤回尚未最终通过的销售变更审批，回到可修正草稿。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 变更单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回变更单详情视图。
pub async fn sales_change_order_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSalesChangeApprovalRequest>,
) -> Result<SalesChangeOrderDetailView> {
    let view = SalesReviewService::new(state.db())
        .cancel_approval(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
