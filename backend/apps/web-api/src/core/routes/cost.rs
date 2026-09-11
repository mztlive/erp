//! 域 D20 `cost` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/cost-entries`、
//! `/admin/cost-allocations`；每条路由统一走 JWT + RBAC（`with_permission`）。

use axum::{
    routing::{get, post},
    Router,
};
use erp_identity::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::cost, middleware::with_permission},
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
            "/actual-profit-loss",
            with_permission(
                get(cost::profit_loss::view),
                rbac,
                cost::profit_loss::view_permission_key(),
            ),
        )
        .route(
            "/actual-profit-loss/period-basis",
            with_permission(
                get(cost::profit_loss::period_basis),
                rbac,
                cost::profit_loss::period_basis_permission_key(),
            ),
        )
        .route(
            "/actual-profit-loss/exports",
            with_permission(
                post(cost::profit_loss::export),
                rbac,
                cost::profit_loss::export_permission_key(),
            ),
        )
        .route(
            "/cost-entries",
            with_permission(
                get(cost::cost_entry_list),
                rbac,
                cost::cost_entry_list_permission_key(),
            ),
        )
        .route(
            "/cost-entries",
            with_permission(
                post(cost::cost_entry_create),
                rbac,
                cost::cost_entry_create_permission_key(),
            ),
        )
        .route(
            "/cost-entries/{id}",
            with_permission(
                get(cost::cost_entry_detail),
                rbac,
                cost::cost_entry_detail_permission_key(),
            ),
        )
        .route(
            "/cost-allocations",
            with_permission(
                get(cost::cost_allocation_list),
                rbac,
                cost::cost_allocation_list_permission_key(),
            ),
        )
}
