//! 域 D16 `fulfillment`：purchase_receipt(+_line)、delivery(+_line)、electronic_delivery、service_fulfillment、customer_acceptance(+_line)、acceptance_fulfillment_allocation（页面：W06、W09）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
