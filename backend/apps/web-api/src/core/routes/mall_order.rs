//! 域 D29 `mall_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/mall-orders`、
//! `/admin/mall-order-facts`；每条路由统一走 JWT + RBAC（`with_permission`），
//! handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::mall_order, middleware::with_permission},
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
            "/mall-orders",
            with_permission(
                get(mall_order::mall_order_list),
                rbac,
                mall_order::mall_order_list_permission_key(),
            ),
        )
        .route(
            "/mall-orders/{id}",
            with_permission(
                get(mall_order::mall_order_detail),
                rbac,
                mall_order::mall_order_detail_permission_key(),
            ),
        )
        .route(
            "/mall-order-facts",
            with_permission(
                get(mall_order::mall_order_fact_list),
                rbac,
                mall_order::mall_order_fact_list_permission_key(),
            ),
        )
        .route(
            "/mall-order-facts",
            with_permission(
                post(mall_order::mall_order_fact_receive),
                rbac,
                mall_order::mall_order_fact_receive_permission_key(),
            ),
        )
}
