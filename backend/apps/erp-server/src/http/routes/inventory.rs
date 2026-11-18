//! 域 D17 `inventory` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/stock-balances`、
//! `/admin/stock-movements`、`/admin/stock-reservations`、`/admin/stock-adjustments`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::inventory;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载库存与库存调整接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new().merge(stock_routes()).merge(adjustment_routes()).merge(adjustment_command_routes())
}

// 为父路由挂载库存余额、流水与预留路由。
fn stock_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/stock-balances",
            permission_route(get(inventory::stock_balance_list), &permissions::inventory::STOCK_BALANCE_LIST),
        )
        .route(
            "/stock-balances/{id}",
            permission_route(
                get(inventory::stock_balance_detail),
                &permissions::inventory::STOCK_BALANCE_DETAIL,
            ),
        )
        .route(
            "/stock-movements",
            permission_route(
                get(inventory::stock_movement_list),
                &permissions::inventory::STOCK_MOVEMENT_LIST,
            ),
        )
        .route(
            "/stock-reservations",
            permission_route(
                get(inventory::stock_reservation_list),
                &permissions::inventory::STOCK_RESERVATION_LIST,
            ),
        )
}

// 为父路由挂载库存调整单查询与草稿路由。
fn adjustment_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/stock-adjustments",
            permission_route(
                get(inventory::stock_adjustment_list),
                &permissions::inventory::STOCK_ADJUSTMENT_LIST,
            ),
        )
        .route(
            "/stock-adjustments",
            permission_route(
                post(inventory::stock_adjustment_create),
                &permissions::inventory::STOCK_ADJUSTMENT_CREATE,
            ),
        )
        .route(
            "/stock-adjustments/{id}",
            permission_route(
                get(inventory::stock_adjustment_detail),
                &permissions::inventory::STOCK_ADJUSTMENT_DETAIL,
            ),
        )
        .route(
            "/stock-adjustments/{id}",
            permission_route(
                put(inventory::stock_adjustment_update),
                &permissions::inventory::STOCK_ADJUSTMENT_UPDATE,
            ),
        )
}

// 为父路由挂载库存调整提交、撤回与记账路由。
fn adjustment_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/stock-adjustments/{id}/submit",
            permission_route(
                post(inventory::stock_adjustment_submit),
                &permissions::inventory::STOCK_ADJUSTMENT_SUBMIT,
            ),
        )
        .route(
            "/stock-adjustments/{id}/submit-result",
            permission_route(
                get(inventory::stock_adjustment_submit_result),
                &permissions::inventory::STOCK_ADJUSTMENT_DETAIL,
            ),
        )
        .route(
            "/stock-adjustments/{id}/cancel-approval",
            permission_route(
                post(inventory::stock_adjustment_cancel_approval),
                &permissions::inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/stock-adjustments/{id}/post",
            permission_route(
                post(inventory::stock_adjustment_post),
                &permissions::inventory::STOCK_ADJUSTMENT_POST,
            ),
        )
}
