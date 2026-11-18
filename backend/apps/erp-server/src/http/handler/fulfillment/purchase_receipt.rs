//! 采购入库 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_fulfillment::dto::{
    CreatePurchaseReceiptRequest, PageView, PostPurchaseReceiptRequest, PurchaseReceiptDetailView,
    PurchaseReceiptListParams, PurchaseReceiptView, UpdatePurchaseReceiptRequest,
};

use super::{process, service};
use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询采购入库单列表（W09 入库视图）。
/// `query` 为分页与筛选参数（`purchase_order_id`/`status` 扁平传递）。
pub async fn purchase_receipt_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PurchaseReceiptListParams>,
) -> Result<PageView<PurchaseReceiptView>> {
    let page = service(&state).purchase_receipt_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询采购入库单详情（表头 + 行）。
/// 返回入库单详情视图。
pub async fn purchase_receipt_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseReceiptDetailView> {
    let view = service(&state).purchase_receipt_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 创建采购入库单（草稿）。
/// 返回新建入库单的响应视图。
pub async fn purchase_receipt_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = process(&state).create_purchase_receipt(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 更新采购入库单（仅草稿；乐观锁冲突返回 409）。
/// `req` 为最终草稿与期望版本。
/// 返回更新后入库单的响应视图。
pub async fn purchase_receipt_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = process(&state).update_purchase_receipt(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 过账采购入库（入库行 + 库存流水 + 余额 + 销售预占同事务，§8.2 第 1 条）。
/// 返回过账后的入库单视图。
pub async fn purchase_receipt_post(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PostPurchaseReceiptRequest>,
) -> Result<PurchaseReceiptView> {
    let view = process(&state).post_purchase_receipt(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}
