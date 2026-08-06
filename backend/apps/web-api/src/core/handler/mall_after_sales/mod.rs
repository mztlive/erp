//! 域 D30 `mall_after_sales` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::mall_after_sales` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    mall_after_sales::{
        AfterSalesRequestListParams, AfterSalesRequestView, MallAfterSalesService,
        MallBalanceRestorationListParams, MallBalanceRestorationView, MallRefundListParams, MallRefundView,
        PageView, ReceiveBalanceRestorationRequest, ReceiveRefundFactRequest, ReceivedFactView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "接收商城退款成功事实",
    resource = "mall_refund",
    action = "submit"
)]
/// 接收商城退款成功事实（`REFUND_SUCCEEDED`，§8.4 第 3 条，幂等）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 退款事实接收请求
///
/// # 返回
/// 返回事实接收结果视图。
pub async fn mall_refund_receive(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ReceiveRefundFactRequest>,
) -> Result<ReceivedFactView> {
    let view = MallAfterSalesService::new(state.db())
        .receive_refund(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "查询商城退款列表",
    resource = "mall_refund",
    action = "list"
)]
/// 查询商城退款列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`mall_order_id`/`after_sales_request_id`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn mall_refund_list(
    State(state): State<AppState>,
    Query(params): Query<MallRefundListParams>,
) -> Result<PageView<MallRefundView>> {
    let page = MallAfterSalesService::new(state.db())
        .mall_refund_list(&params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "接收卡券余额恢复事实",
    resource = "mall_balance_restoration",
    action = "submit"
)]
/// 接收卡券余额恢复事实（`CARD_BALANCE_RESTORED`，§8.4 第 4 条，幂等）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 余额恢复事实接收请求
///
/// # 返回
/// 返回事实接收结果视图。
pub async fn mall_balance_restoration_receive(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ReceiveBalanceRestorationRequest>,
) -> Result<ReceivedFactView> {
    let view = MallAfterSalesService::new(state.db())
        .receive_balance_restoration(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "查询卡券余额恢复列表",
    resource = "mall_balance_restoration",
    action = "list"
)]
/// 查询卡券余额恢复列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`after_sales_request_id`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn mall_balance_restoration_list(
    State(state): State<AppState>,
    Query(params): Query<MallBalanceRestorationListParams>,
) -> Result<PageView<MallBalanceRestorationView>> {
    let page = MallAfterSalesService::new(state.db())
        .mall_balance_restoration_list(&params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "查询商城售后请求列表",
    resource = "mall_after_sales_request",
    action = "list"
)]
/// 查询商城售后请求列表（投影查询）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`mall_id`/`mall_order_id`/`request_type`/`status`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn after_sales_request_list(
    State(state): State<AppState>,
    Query(params): Query<AfterSalesRequestListParams>,
) -> Result<PageView<AfterSalesRequestView>> {
    let page = MallAfterSalesService::new(state.db())
        .after_sales_request_list(&params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}
