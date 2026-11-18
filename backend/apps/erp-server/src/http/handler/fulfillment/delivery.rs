//! 实物发货 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_fulfillment::dto::{
    CreateDeliveryRequest, DeliveryDetailView, DeliveryListParams, DeliveryView, PageView,
    PostDeliveryRequest, UpdateDeliveryRequest,
};

use super::{process, service};
use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询发货单列表（W09 发货视图）。
/// `query` 为分页与筛选参数（`sales_order_id`/`status` 扁平传递）。
pub async fn delivery_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<DeliveryListParams>,
) -> Result<PageView<DeliveryView>> {
    let page = service(&state).delivery_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询发货单详情（表头 + 行）。
/// 返回发货单详情视图。
pub async fn delivery_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<DeliveryDetailView> {
    let view = process(&state).delivery_packing_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 创建发货单（草稿；仓发/直发表头与行归属由实体校验）。
///
/// 发货为 `NO_APPROVAL`：HTTP 创建路径不启动审批、不接受定义 ID。
/// 返回新建发货单的响应视图。
pub async fn delivery_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = process(&state).create_delivery(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 更新发货单（仅草稿；乐观锁冲突返回 409）。
/// `req` 为最终草稿与期望版本。
/// 返回更新后发货单的响应视图。
pub async fn delivery_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = process(&state).update_delivery(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 过账发货（仓发：预占消耗 + 出库流水 + 余额同事务，§8.2 第 2 条；直发：只做门槛校验）。
/// 返回过账后的发货单视图。
pub async fn delivery_post(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostDeliveryRequest>,
) -> Result<DeliveryView> {
    let view = process(&state).post_delivery(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}
