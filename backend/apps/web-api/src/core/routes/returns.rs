//! 域 D21 `returns`：sales_return_case、sales_return_line、purchase_return_order、purchase_return_line、customer_refund、supplier_refund、receipt_reversal、payment_reversal（页面：W05、W09、W11、W12）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
