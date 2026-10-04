//! 采购入库 HTTP 协议适配。

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_fulfillment::dto::{
    CreatePurchaseReceiptRequest, PageView, PostPurchaseReceiptRequest, PurchaseReceiptDetailView,
    PurchaseReceiptListParams, PurchaseReceiptView, UpdatePurchaseReceiptRequest,
};

use super::{process, service};
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询采购入库单列表",
    resource = "purchase_receipt",
    action = "list"
)]
/// 查询采购入库单列表（W09 入库视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`purchase_order_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn purchase_receipt_list(
    State(state): State<AppState>,
    Query(params): Query<PurchaseReceiptListParams>,
) -> Result<PageView<PurchaseReceiptView>> {
    let page = service(&state).purchase_receipt_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询采购入库单详情",
    resource = "purchase_receipt",
    action = "detail"
)]
/// 查询采购入库单详情（表头 + 行）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 入库单主键
///
/// # 返回
/// 返回入库单详情视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn purchase_receipt_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<PurchaseReceiptDetailView> {
    let view = service(&state).purchase_receipt_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "创建采购入库单",
    resource = "purchase_receipt",
    action = "create"
)]
/// 创建采购入库单（草稿）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 行）
///
/// # 返回
/// 返回新建入库单的响应视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn purchase_receipt_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = process(&state).create_purchase_receipt(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "更新采购入库单",
    resource = "purchase_receipt",
    action = "update"
)]
/// 更新采购入库单（仅草稿；乐观锁冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 入库单主键
/// * `req` - 最终草稿与期望版本
///
/// # 返回
/// 返回更新后入库单的响应视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn purchase_receipt_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = process(&state).update_purchase_receipt(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "过账采购入库",
    resource = "purchase_receipt",
    action = "post"
)]
/// 过账采购入库（入库行 + 库存流水 + 余额 + 销售预占同事务，§8.2 第 1 条）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 入库单主键
///
/// # 返回
/// 返回过账后的入库单视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn purchase_receipt_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostPurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = process(&state).post_purchase_receipt(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}
