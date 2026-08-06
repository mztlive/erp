//! 域 D06 `access_control` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/permissions`、
//! `/admin/data-scopes`、`/admin/user-roles`、`/admin/audit-events`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::access_control, middleware::with_permission},
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
            "/permissions",
            with_permission(
                get(access_control::permission_list),
                rbac,
                access_control::permission_list_permission_key(),
            ),
        )
        .route(
            "/permissions",
            with_permission(
                post(access_control::permission_create),
                rbac,
                access_control::permission_create_permission_key(),
            ),
        )
        .route(
            "/permissions/{id}",
            with_permission(
                put(access_control::permission_update),
                rbac,
                access_control::permission_update_permission_key(),
            ),
        )
        .route(
            "/permissions/{id}",
            with_permission(
                delete(access_control::permission_delete),
                rbac,
                access_control::permission_delete_permission_key(),
            ),
        )
        .route(
            "/data-scopes",
            with_permission(
                get(access_control::data_scope_list),
                rbac,
                access_control::data_scope_list_permission_key(),
            ),
        )
        .route(
            "/data-scopes",
            with_permission(
                post(access_control::data_scope_create),
                rbac,
                access_control::data_scope_create_permission_key(),
            ),
        )
        .route(
            "/data-scopes/{id}",
            with_permission(
                delete(access_control::data_scope_delete),
                rbac,
                access_control::data_scope_delete_permission_key(),
            ),
        )
        .route(
            "/user-roles",
            with_permission(
                get(access_control::user_role_list),
                rbac,
                access_control::user_role_list_permission_key(),
            ),
        )
        .route(
            "/user-roles",
            with_permission(
                post(access_control::user_role_create),
                rbac,
                access_control::user_role_create_permission_key(),
            ),
        )
        .route(
            "/user-roles/{id}/revoke",
            with_permission(
                post(access_control::user_role_revoke),
                rbac,
                access_control::user_role_revoke_permission_key(),
            ),
        )
        .route(
            "/audit-events",
            with_permission(
                get(access_control::audit_event_list),
                rbac,
                access_control::audit_event_list_permission_key(),
            ),
        )
}
