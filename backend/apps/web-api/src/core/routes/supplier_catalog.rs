//! 域 D24 `supplier_catalog`：supplier_catalog_product(+_revision、_revision_media)、supplier_catalog_sku、supplier_product_mapping、supplier_catalog_intake_batch、supplier_offering（页面：W21）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
