//! 采购修改原单或办理采购变更时读取可选交付安排范围。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::purchase_center::dto::PurchaseDeliveryOptionsView;

use super::purchase_reads;
use crate::http::errors::Result;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 按销售当前版本读取采购各行的预计交付日上限与收件地址剩余量。
/// 返回采购当前内容引用的商品行交付安排范围。
pub async fn purchase_delivery_options(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseDeliveryOptionsView> {
    Ok(ApiResponse::ok_with_data(purchase_reads(&state).delivery_options(&actor, &id).await?))
}
