//! 域 D20 `cost` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/cost-entries`、
//! `/admin/cost-allocations`；每条路由统一走 JWT + RBAC（`with_permission`）。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

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
