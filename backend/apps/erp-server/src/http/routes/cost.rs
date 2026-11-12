//! 域 D20 `cost` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/cost-entries`、
//! `/admin/cost-allocations`、`/admin/sales-orders/{id}/extra-costs`；
//! 每条路由统一走 JWT + RBAC（`with_permission`）。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::cost;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;

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
            "/actual-profit-loss/history-directory",
            with_permission(get(cost::profit_loss::history_directory), rbac, &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/actual-profit-loss",
            with_permission(get(cost::profit_loss::view), rbac, &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/actual-profit-loss/period-basis",
            with_permission(get(cost::profit_loss::period_basis), rbac, &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/actual-profit-loss/exports",
            with_permission(post(cost::profit_loss::export), rbac, &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/cost-entries",
            with_permission(get(cost::cost_entry_list), rbac, &permissions::cost::ENTRY_LIST),
        )
        .route(
            "/cost-entries",
            with_permission(post(cost::cost_entry_create), rbac, &permissions::cost::ENTRY_CREATE),
        )
        .route(
            "/cost-entries/{id}",
            with_permission(get(cost::cost_entry_detail), rbac, &permissions::cost::ENTRY_DETAIL),
        )
        .route(
            "/cost-allocations",
            with_permission(get(cost::cost_allocation_list), rbac, &permissions::cost::ALLOCATION_LIST),
        )
        .merge(extra_cost_routes(rbac))
}

/// 销售单附加费用路由：列表、财务登记与冲减。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的附加费用路由集合。
fn extra_cost_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/extra-costs",
            with_permission(
                get(cost::extra_cost::sales_extra_cost_list),
                rbac,
                &permissions::cost::ENTRY_LIST,
            ),
        )
        .route(
            "/sales-orders/{id}/extra-costs",
            with_permission(
                post(cost::extra_cost::sales_extra_cost_register),
                rbac,
                &permissions::cost::ENTRY_CREATE,
            ),
        )
        .route(
            "/sales-orders/{id}/extra-costs/{cost_entry_id}/reductions",
            with_permission(
                post(cost::extra_cost::sales_extra_cost_reduce),
                rbac,
                &permissions::cost::ENTRY_CREATE,
            ),
        )
}
