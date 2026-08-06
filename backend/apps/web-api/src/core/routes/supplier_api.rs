//! 域 D25 `supplier_api` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-api-connections`、
//! `/admin/supplier-api-capabilities`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier_api, middleware::with_permission},
};

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections",
            with_permission(
                get(supplier_api::supplier_api_connection_list),
                rbac,
                supplier_api::supplier_api_connection_list_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections",
            with_permission(
                post(supplier_api::supplier_api_connection_create),
                rbac,
                supplier_api::supplier_api_connection_create_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}",
            with_permission(
                get(supplier_api::supplier_api_connection_detail),
                rbac,
                supplier_api::supplier_api_connection_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}",
            with_permission(
                put(supplier_api::supplier_api_connection_update),
                rbac,
                supplier_api::supplier_api_connection_update_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/capabilities",
            with_permission(
                put(supplier_api::supplier_api_capabilities_replace),
                rbac,
                supplier_api::supplier_api_capabilities_replace_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/health-check",
            with_permission(
                post(supplier_api::supplier_api_connection_health_check),
                rbac,
                supplier_api::supplier_api_connection_health_check_permission_key(),
            ),
        )
        .route(
            "/supplier-api-capabilities",
            with_permission(
                get(supplier_api::supplier_api_capability_list),
                rbac,
                supplier_api::supplier_api_capability_list_permission_key(),
            ),
        )
}
