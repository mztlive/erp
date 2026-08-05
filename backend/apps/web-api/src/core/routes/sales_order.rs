//! 域 D13 `sales_order`：sales_order(+_line)、sales_order_working_copy、sales_order_submission、sales_order_revision、goods_service_line_revision、voucher_line_revision（页面：W05）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
