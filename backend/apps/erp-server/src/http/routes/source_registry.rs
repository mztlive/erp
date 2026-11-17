//! 域 D01 `source_registry` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/source-systems`、
//! `/admin/external-identity-maps`；每条路由统一走
//! JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::source_registry;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回声明公共权限目录信息的路由集合。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/source-systems",
            permission_route(
                get(source_registry::source_system_list),
                &permissions::source_registry::SOURCE_SYSTEM_LIST,
            ),
        )
        .route(
            "/source-systems",
            permission_route(
                post(source_registry::source_system_create),
                &permissions::source_registry::SOURCE_SYSTEM_CREATE,
            ),
        )
        .route(
            "/source-systems/{id}",
            permission_route(
                put(source_registry::source_system_update),
                &permissions::source_registry::SOURCE_SYSTEM_UPDATE,
            ),
        )
        .route(
            "/external-identity-maps",
            permission_route(
                get(source_registry::external_identity_map_list),
                &permissions::source_registry::EXTERNAL_IDENTITY_MAP_LIST,
            ),
        )
        .route(
            "/external-identity-maps",
            permission_route(
                post(source_registry::external_identity_map_create),
                &permissions::source_registry::EXTERNAL_IDENTITY_MAP_CREATE,
            ),
        )
}
