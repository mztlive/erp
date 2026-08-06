//! 域 D27 `projection` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-order-projections`、
//! `/admin/sales-order-projection-deliveries`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::projection, middleware::with_permission},
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
            "/sales-order-projections",
            with_permission(
                get(projection::sales_order_projection_list),
                rbac,
                projection::sales_order_projection_list_permission_key(),
            ),
        )
        .route(
            "/sales-order-projections",
            with_permission(
                post(projection::sales_order_projection_create),
                rbac,
                projection::sales_order_projection_create_permission_key(),
            ),
        )
        .route(
            "/sales-order-projections/{id}",
            with_permission(
                get(projection::sales_order_projection_detail),
                rbac,
                projection::sales_order_projection_detail_permission_key(),
            ),
        )
        .route(
            "/sales-order-projections/{id}/revisions",
            with_permission(
                post(projection::sales_order_projection_revision_create),
                rbac,
                projection::sales_order_projection_revision_create_permission_key(),
            ),
        )
        .route(
            "/sales-order-projections/{id}/revisions",
            with_permission(
                get(projection::sales_order_projection_revision_list),
                rbac,
                projection::sales_order_projection_revision_list_permission_key(),
            ),
        )
        .route(
            "/sales-order-projections/{id}/revisions/{revision_no}/deliver",
            with_permission(
                post(projection::sales_order_projection_delivery_submit),
                rbac,
                projection::sales_order_projection_delivery_submit_permission_key(),
            ),
        )
        .route(
            "/sales-order-projection-deliveries",
            with_permission(
                get(projection::sales_order_projection_delivery_list),
                rbac,
                projection::sales_order_projection_delivery_list_permission_key(),
            ),
        )
}
