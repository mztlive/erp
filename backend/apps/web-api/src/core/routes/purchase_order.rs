//! 域 D15 `purchase_order`：purchase_order、purchase_order_submission、purchase_order_revision、purchase_line_sales_allocation、purchase_change_order 等（页面：W08）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
