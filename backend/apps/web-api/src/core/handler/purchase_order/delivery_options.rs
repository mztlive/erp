//! 采购修改原单或办理采购变更时读取可选交付安排范围。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::purchase_center::dto::PurchaseDeliveryOptionsView;

use super::purchase_reads;
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "保存采购草稿",
    resource = "purchase_order",
    action = "update"
)]
/// 按销售当前版本读取采购各行的预计交付日上限与收件地址剩余量。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 采购单 ID
///
/// # 返回
/// 返回采购当前内容引用的商品行交付安排范围。
pub async fn purchase_delivery_options(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseDeliveryOptionsView> {
    Ok(ApiResponse::ok_with_data(purchase_reads(&state).delivery_options(&actor, &id).await?))
}
