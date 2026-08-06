//! 域 D30 `mall_after_sales` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/mall-refunds`、
//! `/admin/mall-balance-restorations`、`/admin/mall-after-sales-requests`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::mall_after_sales, middleware::with_permission},
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
            "/mall-refunds",
            with_permission(
                post(mall_after_sales::mall_refund_receive),
                rbac,
                mall_after_sales::mall_refund_receive_permission_key(),
            ),
        )
        .route(
            "/mall-refunds",
            with_permission(
                get(mall_after_sales::mall_refund_list),
                rbac,
                mall_after_sales::mall_refund_list_permission_key(),
            ),
        )
        .route(
            "/mall-balance-restorations",
            with_permission(
                post(mall_after_sales::mall_balance_restoration_receive),
                rbac,
                mall_after_sales::mall_balance_restoration_receive_permission_key(),
            ),
        )
        .route(
            "/mall-balance-restorations",
            with_permission(
                get(mall_after_sales::mall_balance_restoration_list),
                rbac,
                mall_after_sales::mall_balance_restoration_list_permission_key(),
            ),
        )
        .route(
            "/mall-after-sales-requests",
            with_permission(
                get(mall_after_sales::after_sales_request_list),
                rbac,
                mall_after_sales::after_sales_request_list_permission_key(),
            ),
        )
}
