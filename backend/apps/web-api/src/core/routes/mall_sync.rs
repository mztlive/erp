//! 域 D23 `mall_sync`：mall_sales_sync_job、mall_sales_sync_cursor、mall_sales_order_snapshot、mall_sales_reconciliation_job(+_item)、master_mapping_task（页面：W17）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
