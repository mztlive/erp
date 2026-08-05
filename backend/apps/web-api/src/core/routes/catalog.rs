//! 域 D10 `catalog`：product_category、product_brand、unit_of_measure、sku_attribute、product(+_revision)、sku(+_revision)、voucher_category_profile_revision 等（页面：W14）。P0 骨架占位；P3 填充路由与权限挂载（管理端路由必须带 #[permission]）。

use axum::Router;

use crate::app_state::AppState;

/// 返回本域管理端路由集合（P3 填充）。
pub fn routes(_rbac: &services::iam::SharedRbacService) -> Router<AppState> {
    Router::<AppState>::new()
}
