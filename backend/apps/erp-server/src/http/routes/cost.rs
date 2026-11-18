//! 域 D20 `cost` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/cost-entries`、
//! `/admin/cost-allocations`、`/admin/sales-orders/{id}/extra-costs`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::cost;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载成本与分配接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/actual-profit-loss/history-directory",
            permission_route(get(cost::profit_loss::history_directory), &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/actual-profit-loss",
            permission_route(get(cost::profit_loss::view), &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/actual-profit-loss/period-basis",
            permission_route(get(cost::profit_loss::period_basis), &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/actual-profit-loss/exports",
            permission_route(post(cost::profit_loss::export), &permissions::cost::ENTRY_LIST),
        )
        .route("/cost-entries", permission_route(get(cost::cost_entry_list), &permissions::cost::ENTRY_LIST))
        .route(
            "/cost-entries",
            permission_route(post(cost::cost_entry_create), &permissions::cost::ENTRY_CREATE),
        )
        .route(
            "/cost-entries/{id}",
            permission_route(get(cost::cost_entry_detail), &permissions::cost::ENTRY_DETAIL),
        )
        .route(
            "/cost-allocations",
            permission_route(get(cost::cost_allocation_list), &permissions::cost::ALLOCATION_LIST),
        )
        .merge(extra_cost_routes())
}

// 销售单附加费用路由：列表、财务登记与冲减。
// 返回声明公共权限目录信息的附加费用路由集合。
fn extra_cost_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/extra-costs",
            permission_route(get(cost::extra_cost::sales_extra_cost_list), &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/sales-orders/{id}/extra-costs",
            permission_route(
                post(cost::extra_cost::sales_extra_cost_register),
                &permissions::cost::ENTRY_CREATE,
            ),
        )
        .route(
            "/sales-orders/{id}/extra-costs/{cost_entry_id}/reductions",
            permission_route(
                post(cost::extra_cost::sales_extra_cost_reduce),
                &permissions::cost::ENTRY_CREATE,
            ),
        )
}
