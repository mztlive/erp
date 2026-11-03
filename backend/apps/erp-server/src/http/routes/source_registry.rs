//! 域 D01 `source_registry` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/source-systems`、
//! `/admin/external-identity-maps`；每条路由统一走
//! JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::http::handler::source_registry;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/source-systems",
            with_permission(
                get(source_registry::source_system_list),
                rbac,
                permissions::source_registry::SOURCE_SYSTEM_LIST,
            ),
        )
        .route(
            "/source-systems",
            with_permission(
                post(source_registry::source_system_create),
                rbac,
                permissions::source_registry::SOURCE_SYSTEM_CREATE,
            ),
        )
        .route(
            "/source-systems/{id}",
            with_permission(
                put(source_registry::source_system_update),
                rbac,
                permissions::source_registry::SOURCE_SYSTEM_UPDATE,
            ),
        )
        .route(
            "/external-identity-maps",
            with_permission(
                get(source_registry::external_identity_map_list),
                rbac,
                permissions::source_registry::EXTERNAL_IDENTITY_MAP_LIST,
            ),
        )
        .route(
            "/external-identity-maps",
            with_permission(
                post(source_registry::external_identity_map_create),
                rbac,
                permissions::source_registry::EXTERNAL_IDENTITY_MAP_CREATE,
            ),
        )
}
