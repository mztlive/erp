//! 域 D08 `customer` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/customers`、
//! `/admin/customers/{id}/assignments` 等；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::customer, middleware::with_permission},
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
            "/customer-profiles",
            with_permission(
                post(customer::customer_profile_create),
                rbac,
                customer::customer_profile_create_permission_key(),
            ),
        )
        .route(
            "/customer-profiles/{id}",
            with_permission(
                get(customer::customer_profile_detail),
                rbac,
                customer::customer_profile_detail_permission_key(),
            ),
        )
        .route(
            "/customer-profiles/{id}/related-summary",
            with_permission(
                get(customer::customer_center_related),
                rbac,
                customer::customer_center_related_permission_key(),
            ),
        )
        .route(
            "/customer-profiles/{id}/receivable-summary",
            with_permission(
                get(customer::customer_center_receivable),
                rbac,
                customer::customer_center_receivable_permission_key(),
            ),
        )
        .route(
            "/customer-profiles/{id}",
            with_permission(
                put(customer::customer_profile_update),
                rbac,
                customer::customer_profile_update_permission_key(),
            ),
        )
        .route(
            "/customer-profile-commands/{idempotency_key}",
            with_permission(
                get(customer::customer_profile_command_detail),
                rbac,
                customer::customer_profile_command_detail_permission_key(),
            ),
        )
        .route(
            "/customer-sensitive-fields/reveal",
            with_permission(
                post(customer::customer_sensitive_reveal),
                rbac,
                customer::customer_sensitive_reveal_permission_key(),
            ),
        )
        .route(
            "/customers",
            with_permission(
                get(customer::customer_list),
                rbac,
                customer::customer_list_permission_key(),
            ),
        )
        .route(
            "/customers/all-authorized",
            with_permission(
                get(customer::customer_all_authorized_list),
                rbac,
                customer::customer_all_authorized_list_permission_key(),
            ),
        )
        .route(
            "/customers",
            with_permission(
                post(customer::customer_create),
                rbac,
                customer::customer_create_permission_key(),
            ),
        )
        .route(
            "/customers/{id}",
            with_permission(
                get(customer::customer_detail),
                rbac,
                customer::customer_detail_permission_key(),
            ),
        )
        .route(
            "/customers/{id}",
            with_permission(
                put(customer::customer_update),
                rbac,
                customer::customer_update_permission_key(),
            ),
        )
        .route(
            "/customers/{id}",
            with_permission(
                delete(customer::customer_delete),
                rbac,
                customer::customer_delete_permission_key(),
            ),
        )
        .route(
            "/customers/{id}/assignments",
            with_permission(
                get(customer::customer_assignment_list),
                rbac,
                customer::customer_assignment_list_permission_key(),
            ),
        )
        .route(
            "/customers/{id}/assignments",
            with_permission(
                post(customer::customer_assignment_apply),
                rbac,
                customer::customer_assignment_apply_permission_key(),
            ),
        )
}
