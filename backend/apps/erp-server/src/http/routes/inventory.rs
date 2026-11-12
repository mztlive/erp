//! 域 D17 `inventory` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/stock-balances`、
//! `/admin/stock-movements`、`/admin/stock-reservations`、`/admin/stock-adjustments`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::inventory;
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
            "/stock-balances",
            with_permission(
                get(inventory::stock_balance_list),
                rbac,
                &permissions::inventory::STOCK_BALANCE_LIST,
            ),
        )
        .route(
            "/stock-balances/{id}",
            with_permission(
                get(inventory::stock_balance_detail),
                rbac,
                &permissions::inventory::STOCK_BALANCE_DETAIL,
            ),
        )
        .route(
            "/stock-movements",
            with_permission(
                get(inventory::stock_movement_list),
                rbac,
                &permissions::inventory::STOCK_MOVEMENT_LIST,
            ),
        )
        .route(
            "/stock-reservations",
            with_permission(
                get(inventory::stock_reservation_list),
                rbac,
                &permissions::inventory::STOCK_RESERVATION_LIST,
            ),
        )
        .route(
            "/stock-adjustments",
            with_permission(
                get(inventory::stock_adjustment_list),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_LIST,
            ),
        )
        .route(
            "/stock-adjustments",
            with_permission(
                post(inventory::stock_adjustment_create),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_CREATE,
            ),
        )
        .route(
            "/stock-adjustments/{id}",
            with_permission(
                get(inventory::stock_adjustment_detail),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_DETAIL,
            ),
        )
        .route(
            "/stock-adjustments/{id}",
            with_permission(
                put(inventory::stock_adjustment_update),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_UPDATE,
            ),
        )
        .route(
            "/stock-adjustments/{id}/submit",
            with_permission(
                post(inventory::stock_adjustment_submit),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_SUBMIT,
            ),
        )
        .route(
            "/stock-adjustments/{id}/submit-result",
            with_permission(
                get(inventory::stock_adjustment_submit_result),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_DETAIL,
            ),
        )
        .route(
            "/stock-adjustments/{id}/cancel-approval",
            with_permission(
                post(inventory::stock_adjustment_cancel_approval),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/stock-adjustments/{id}/post",
            with_permission(
                post(inventory::stock_adjustment_post),
                rbac,
                &permissions::inventory::STOCK_ADJUSTMENT_POST,
            ),
        )
}
