//! 实物发货 HTTP 协议适配。

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_fulfillment::dto::{
    CreateDeliveryRequest, DeliveryDetailView, DeliveryListParams, DeliveryView, PageView,
    PostDeliveryRequest, UpdateDeliveryRequest,
};

use super::{process, service};
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询发货单列表",
    resource = "delivery",
    action = "list"
)]
/// 查询发货单列表（W09 发货视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn delivery_list(
    State(state): State<AppState>,
    Query(params): Query<DeliveryListParams>,
) -> Result<PageView<DeliveryView>> {
    let page = service(&state).delivery_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询发货单详情",
    resource = "delivery",
    action = "detail"
)]
/// 查询发货单详情（表头 + 行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 发货单主键
///
/// # 返回
/// 返回发货单详情视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn delivery_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<DeliveryDetailView> {
    let view = service(&state).delivery_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建发货单",
    resource = "delivery",
    action = "create"
)]
/// 创建发货单（草稿；仓发/直发表头与行归属由实体校验）。
///
/// 发货为 `NO_APPROVAL`：HTTP 创建路径不启动审批、不接受定义 ID。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 行）
///
/// # 返回
/// 返回新建发货单的响应视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn delivery_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = process(&state).create_delivery(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "更新发货单",
    resource = "delivery",
    action = "update"
)]
/// 更新发货单（仅草稿；乐观锁冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发货单主键
/// * `req` - 最终草稿与期望版本
///
/// # 返回
/// 返回更新后发货单的响应视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn delivery_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = process(&state).update_delivery(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "过账发货",
    resource = "delivery",
    action = "post"
)]
/// 过账发货（仓发：预占消耗 + 出库流水 + 余额同事务，§8.2 第 2 条；直发：只做门槛校验）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 发货单主键
///
/// # 返回
/// 返回过账后的发货单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn delivery_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = process(&state).post_delivery(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}
