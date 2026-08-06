//! 域 D17 `inventory` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/stock-balances`、
//! `/admin/stock-movements`、`/admin/stock-reservations`、`/admin/stock-adjustments`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::inventory, middleware::with_permission},
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
            "/stock-balances",
            with_permission(
                get(inventory::stock_balance_list),
                rbac,
                inventory::stock_balance_list_permission_key(),
            ),
        )
        .route(
            "/stock-balances/{id}",
            with_permission(
                get(inventory::stock_balance_detail),
                rbac,
                inventory::stock_balance_detail_permission_key(),
            ),
        )
        .route(
            "/stock-movements",
            with_permission(
                get(inventory::stock_movement_list),
                rbac,
                inventory::stock_movement_list_permission_key(),
            ),
        )
        .route(
            "/stock-reservations",
            with_permission(
                get(inventory::stock_reservation_list),
                rbac,
                inventory::stock_reservation_list_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments",
            with_permission(
                get(inventory::stock_adjustment_list),
                rbac,
                inventory::stock_adjustment_list_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments",
            with_permission(
                post(inventory::stock_adjustment_create),
                rbac,
                inventory::stock_adjustment_create_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments/{id}",
            with_permission(
                get(inventory::stock_adjustment_detail),
                rbac,
                inventory::stock_adjustment_detail_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments/{id}",
            with_permission(
                put(inventory::stock_adjustment_update),
                rbac,
                inventory::stock_adjustment_update_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments/{id}/submit",
            with_permission(
                post(inventory::stock_adjustment_submit),
                rbac,
                inventory::stock_adjustment_submit_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments/{id}/approve",
            with_permission(
                post(inventory::stock_adjustment_approve),
                rbac,
                inventory::stock_adjustment_approve_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments/{id}/reject",
            with_permission(
                post(inventory::stock_adjustment_reject),
                rbac,
                inventory::stock_adjustment_reject_permission_key(),
            ),
        )
        .route(
            "/stock-adjustments/{id}/post",
            with_permission(
                post(inventory::stock_adjustment_post),
                rbac,
                inventory::stock_adjustment_post_permission_key(),
            ),
        )
}
