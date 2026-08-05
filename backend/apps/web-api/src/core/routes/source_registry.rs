//! 域 D01 `source_registry` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/source-systems`、
//! `/admin/external-identity-maps`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::source_registry, middleware::with_permission},
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
            "/source-systems",
            with_permission(
                get(source_registry::source_system_list),
                rbac,
                source_registry::source_system_list_permission_key(),
            ),
        )
        .route(
            "/source-systems",
            with_permission(
                post(source_registry::source_system_create),
                rbac,
                source_registry::source_system_create_permission_key(),
            ),
        )
        .route(
            "/source-systems/{id}",
            with_permission(
                put(source_registry::source_system_update),
                rbac,
                source_registry::source_system_update_permission_key(),
            ),
        )
        .route(
            "/external-identity-maps",
            with_permission(
                get(source_registry::external_identity_map_list),
                rbac,
                source_registry::external_identity_map_list_permission_key(),
            ),
        )
        .route(
            "/external-identity-maps",
            with_permission(
                post(source_registry::external_identity_map_create),
                rbac,
                source_registry::external_identity_map_create_permission_key(),
            ),
        )
}
