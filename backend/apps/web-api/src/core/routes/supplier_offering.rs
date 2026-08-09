//! 供应商供给管理路由。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier_offering, middleware::with_permission},
};

/// 构建供应商供给管理路由。
///
/// # 参数
/// * `rbac` - 授权服务
///
/// # 返回
/// 返回已挂载权限门禁的路由。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-offerings",
            with_permission(
                get(supplier_offering::list),
                rbac,
                supplier_offering::list_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings",
            with_permission(
                post(supplier_offering::create),
                rbac,
                supplier_offering::create_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/{id}/revisions",
            with_permission(
                post(supplier_offering::revise),
                rbac,
                supplier_offering::revise_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/{id}/availability",
            with_permission(
                post(supplier_offering::update_availability),
                rbac,
                supplier_offering::update_availability_permission_key(),
            ),
        )
}
