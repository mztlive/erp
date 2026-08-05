//! 域 D14 `sales_review`：sales_order_review、procurement_confirmation(+_line)、sales_change_order、sales_change_submission、sales_change_review（页面：W05、W07）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
