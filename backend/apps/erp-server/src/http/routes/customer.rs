//! 域 D08 `customer` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/customers`、
//! `/admin/customers/{id}/assignments` 等；每条路由统一走
//! JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post, put};
use erp_permissions as permissions;

use crate::http::handler::customer;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载客户档案与经营查询接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(profile_query_routes())
        .merge(profile_command_routes())
        .merge(customer_routes())
        .merge(import_routes())
        .merge(assignment_routes())
}

// 为父路由挂载客户资料与业务汇总读取路由。
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

// 为父路由挂载客户资料命令与敏感字段路由。
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

// 为父路由挂载客户档案查询与维护路由。
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

fn import_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customers/import/jobs",
            permission_route(
                post(customer::import::submit).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
                &permissions::customer::CREATE,
            ),
        )
        .route(
            "/customers/import/jobs/{id}/result-rows",
            permission_route(get(customer::import::result_rows), &permissions::customer::CREATE),
        )
}

// 为父路由挂载客户负责人分配路由。
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
