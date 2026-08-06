//! 域 D29 `mall_order` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::mall_order` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    mall_order::{
        MallOrderDetailView, MallOrderFactListParams, MallOrderFactView, MallOrderListParams,
        MallOrderListRow, MallOrderService, PageView, ReceiveMallOrderFactRequest, ReceivedFactView,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "查询商城消费订单列表",
    resource = "mall_order",
    action = "list"
)]
/// 查询商城消费订单列表（W25 列表页）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`q`/`mall_id`/`external_order_no`/`customer_id`/
///   `fulfillment_chain`/`attribution_status`/`paid_at_from`/`paid_at_to`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn mall_order_list(
    State(state): State<AppState>,
    Query(params): Query<MallOrderListParams>,
) -> Result<PageView<MallOrderListRow>> {
    let page = MallOrderService::new(state.db()).mall_order_list(&params).await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "查询商城消费订单详情",
    resource = "mall_order",
    action = "detail"
)]
/// 查询商城消费订单详情（W25 对象中心）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 商城订单 ID
///
/// # 返回
/// 返回订单详情视图。
pub async fn mall_order_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<MallOrderDetailView> {
    let view = MallOrderService::new(state.db()).mall_order_detail(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "查询商城关键事实列表",
    resource = "mall_order_fact",
    action = "list"
)]
/// 查询商城关键事实列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`mall_id`/`fact_type`/`processing_status`/
///   `after_sales_request_id`）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn mall_order_fact_list(
    State(state): State<AppState>,
    Query(params): Query<MallOrderFactListParams>,
) -> Result<PageView<MallOrderFactView>> {
    let page = MallOrderService::new(state.db())
        .mall_order_fact_list(&params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "商城消费订单",
    group_desc = "商城消费订单追溯与关键事实接收",
    desc = "接收商城支付/取消/完成关键事实",
    resource = "mall_order_fact",
    action = "submit"
)]
/// 接收商城关键事实（支付入账/取消/完成；退款与余额恢复走售后域接口）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 事实接收请求（`business_fact_key`/`inbox_message_id` 幂等）
///
/// # 返回
/// 返回事实接收结果视图（幂等命中时返回既有事实）。
pub async fn mall_order_fact_receive(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ReceiveMallOrderFactRequest>,
) -> Result<ReceivedFactView> {
    let view = MallOrderService::new(state.db())
        .receive_fact(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
