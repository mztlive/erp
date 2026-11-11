//! 域 D08 `customer` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/customers`、
//! `/admin/customers/{id}/assignments` 等；每条路由统一走
//! JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::customer;
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
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-profiles",
            with_permission(
                post(customer::customer_profile_create),
                rbac,
                permissions::customer::PROFILE_CREATE,
            ),
        )
        .route(
            "/customer-profiles/{id}",
            with_permission(
                get(customer::customer_profile_detail),
                rbac,
                permissions::customer::PROFILE_DETAIL,
            ),
        )
        .route(
            "/customer-profiles/{id}/related-summary",
            with_permission(
                get(customer::customer_center_related),
                rbac,
                permissions::customer::CENTER_RELATED,
            ),
        )
        .route(
            "/customer-profiles/{id}/receivable-summary",
            with_permission(
                get(customer::customer_center_receivable),
                rbac,
                permissions::customer::CENTER_RECEIVABLE,
            ),
        )
        .route(
            "/customer-profiles/{id}",
            with_permission(
                put(customer::customer_profile_update),
                rbac,
                permissions::customer::PROFILE_UPDATE,
            ),
        )
        .route(
            "/customer-profile-commands/{idempotency_key}",
            with_permission(
                get(customer::customer_profile_command_detail),
                rbac,
                permissions::customer::PROFILE_COMMAND_DETAIL,
            ),
        )
        .route(
            "/customer-sensitive-fields/reveal",
            with_permission(
                post(customer::customer_sensitive_reveal),
                rbac,
                permissions::customer::SENSITIVE_REVEAL,
            ),
        )
        .route("/customers", with_permission(get(customer::customer_list), rbac, permissions::customer::LIST))
        .route(
            "/customers/all-authorized",
            with_permission(
                get(customer::customer_all_authorized_list),
                rbac,
                permissions::customer::ALL_AUTHORIZED_LIST,
            ),
        )
        .route(
            "/customers",
            with_permission(post(customer::customer_create), rbac, permissions::customer::CREATE),
        )
        .route(
            "/customers/{id}",
            with_permission(get(customer::customer_detail), rbac, permissions::customer::DETAIL),
        )
        .route(
            "/customers/{id}",
            with_permission(put(customer::customer_update), rbac, permissions::customer::UPDATE),
        )
        .route(
            "/customers/{id}",
            with_permission(delete(customer::customer_delete), rbac, permissions::customer::DELETE),
        )
        .route(
            "/customers/{id}/assignments",
            with_permission(
                get(customer::customer_assignment_list),
                rbac,
                permissions::customer::ASSIGNMENT_LIST,
            ),
        )
        .route(
            "/customers/{id}/assignments",
            with_permission(
                post(customer::customer_assignment_apply),
                rbac,
                permissions::customer::ASSIGNMENT_APPLY,
            ),
        )
}
