//! 域 D29 `mall_order`：mall_order_fact、mall_order_cancel_fact、mall_order_completion_fact、mall_order、mall_order_item、mall_payment_source、mall_consumption_entry 等（页面：W25、W28）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
