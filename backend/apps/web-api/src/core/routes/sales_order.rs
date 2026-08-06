//! 域 D13 `sales_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-orders`；每条路由
//! 统一走 JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::sales_order, middleware::with_permission},
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
            "/sales-orders",
            with_permission(
                get(sales_order::sales_order_list),
                rbac,
                sales_order::sales_order_list_permission_key(),
            ),
        )
        .route(
            "/sales-orders",
            with_permission(
                post(sales_order::sales_order_create),
                rbac,
                sales_order::sales_order_create_permission_key(),
            ),
        )
        .route(
            "/sales-orders/{id}",
            with_permission(
                get(sales_order::sales_order_detail),
                rbac,
                sales_order::sales_order_detail_permission_key(),
            ),
        )
        .route(
            "/sales-orders/{id}/working-copy",
            with_permission(
                put(sales_order::sales_order_save_working_copy),
                rbac,
                sales_order::sales_order_save_working_copy_permission_key(),
            ),
        )
        .route(
            "/sales-orders/{id}/submit",
            with_permission(
                post(sales_order::sales_order_submit),
                rbac,
                sales_order::sales_order_submit_permission_key(),
            ),
        )
        .route(
            "/sales-orders/{id}/void",
            with_permission(
                post(sales_order::sales_order_void),
                rbac,
                sales_order::sales_order_void_permission_key(),
            ),
        )
}
