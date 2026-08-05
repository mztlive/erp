//! 域 D32 `supplier_fulfillment`：supplier_fulfillment_order、supplier_fulfillment_item、supplier_order_action(+_line)、supplier_order_status_history、supplier_refund_fact、supplier_refund_allocation（页面：W26）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
