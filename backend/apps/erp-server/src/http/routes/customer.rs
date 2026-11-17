//! 域 D08 `customer` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/customers`、
//! `/admin/customers/{id}/assignments` 等；每条路由统一走
//! JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_permissions as permissions;

use crate::http::handler::customer;
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
        .merge(profile_query_routes())
        .merge(profile_command_routes())
        .merge(customer_routes())
        .merge(assignment_routes())
}

/// 客户资料与业务汇总读取路由。
fn profile_query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-profiles",
            permission_route(post(customer::customer_profile_create), &permissions::customer::CREATE),
        )
        .route(
            "/customer-profiles/{id}",
            permission_route(get(customer::customer_profile_detail), &permissions::customer::DETAIL),
        )
        .route(
            "/customer-profiles/{id}/related-summary",
            permission_route(get(customer::customer_center_related), &permissions::customer::DETAIL),
        )
        .route(
            "/customer-profiles/{id}/receivable-summary",
            permission_route(get(customer::customer_center_receivable), &permissions::customer::DETAIL),
        )
}

/// 客户资料命令与敏感字段路由。
fn profile_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-profiles/{id}",
            permission_route(put(customer::customer_profile_update), &permissions::customer::UPDATE),
        )
        .route(
            "/customer-profile-commands/{idempotency_key}",
            permission_route(get(customer::customer_profile_command_detail), &permissions::customer::DETAIL),
        )
        .route(
            "/customer-sensitive-fields/reveal",
            permission_route(
                post(customer::customer_sensitive_reveal),
                &permissions::customer::SENSITIVE_REVEAL,
            ),
        )
}

/// 客户档案查询与维护路由。
fn customer_routes() -> Router<HttpState> {
    Router::new()
        .route("/customers", permission_route(get(customer::customer_list), &permissions::customer::LIST))
        .route(
            "/customers/all-authorized",
            permission_route(
                get(customer::customer_all_authorized_list),
                &permissions::customer::ALL_AUTHORIZED_LIST,
            ),
        )
        .route(
            "/customers",
            permission_route(post(customer::customer_create), &permissions::customer::CREATE),
        )
        .route(
            "/customers/{id}",
            permission_route(get(customer::customer_detail), &permissions::customer::DETAIL),
        )
        .route(
            "/customers/{id}",
            permission_route(put(customer::customer_update), &permissions::customer::UPDATE),
        )
        .route(
            "/customers/{id}",
            permission_route(delete(customer::customer_delete), &permissions::customer::DELETE),
        )
}

/// 客户负责人分配路由。
fn assignment_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customers/{id}/assignments",
            permission_route(
                get(customer::customer_assignment_list),
                &permissions::customer::ASSIGNMENT_LIST,
            ),
        )
        .route(
            "/customers/{id}/assignments",
            permission_route(
                post(customer::customer_assignment_apply),
                &permissions::customer::ASSIGNMENT_APPLY,
            ),
        )
}
