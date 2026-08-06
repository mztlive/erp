//! 域 D21 `returns` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-return-cases`、
//! `/admin/purchase-return-orders`、`/admin/customer-refunds`、
//! `/admin/supplier-refunds`、`/admin/receipt-reversals`、`/admin/payment-reversals`；
//! 每条路由统一走 JWT + RBAC（`with_permission`）。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::returns, middleware::with_permission},
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
            "/sales-return-cases",
            with_permission(
                get(returns::sales_return_case_list),
                rbac,
                returns::sales_return_case_list_permission_key(),
            ),
        )
        .route(
            "/sales-return-cases",
            with_permission(
                post(returns::sales_return_case_create),
                rbac,
                returns::sales_return_case_create_permission_key(),
            ),
        )
        .route(
            "/sales-return-cases/{id}",
            with_permission(
                get(returns::sales_return_case_detail),
                rbac,
                returns::sales_return_case_detail_permission_key(),
            ),
        )
        .route(
            "/purchase-return-orders",
            with_permission(
                get(returns::purchase_return_order_list),
                rbac,
                returns::purchase_return_order_list_permission_key(),
            ),
        )
        .route(
            "/purchase-return-orders",
            with_permission(
                post(returns::purchase_return_order_create),
                rbac,
                returns::purchase_return_order_create_permission_key(),
            ),
        )
        .route(
            "/purchase-return-orders/{id}",
            with_permission(
                get(returns::purchase_return_order_detail),
                rbac,
                returns::purchase_return_order_detail_permission_key(),
            ),
        )
        .route(
            "/customer-refunds",
            with_permission(
                get(returns::customer_refund_list),
                rbac,
                returns::customer_refund_list_permission_key(),
            ),
        )
        .route(
            "/customer-refunds",
            with_permission(
                post(returns::customer_refund_create),
                rbac,
                returns::customer_refund_create_permission_key(),
            ),
        )
        .route(
            "/customer-refunds/{id}",
            with_permission(
                get(returns::customer_refund_detail),
                rbac,
                returns::customer_refund_detail_permission_key(),
            ),
        )
        .route(
            "/customer-refunds/{id}/post",
            with_permission(
                post(returns::customer_refund_post),
                rbac,
                returns::customer_refund_post_permission_key(),
            ),
        )
        .route(
            "/supplier-refunds",
            with_permission(
                post(returns::supplier_refund_create),
                rbac,
                returns::supplier_refund_create_permission_key(),
            ),
        )
        .route(
            "/supplier-refunds/{id}/post",
            with_permission(
                post(returns::supplier_refund_post),
                rbac,
                returns::supplier_refund_post_permission_key(),
            ),
        )
        .route(
            "/receipt-reversals",
            with_permission(
                post(returns::receipt_reversal_create),
                rbac,
                returns::receipt_reversal_create_permission_key(),
            ),
        )
        .route(
            "/receipt-reversals/{id}/post",
            with_permission(
                post(returns::receipt_reversal_post),
                rbac,
                returns::receipt_reversal_post_permission_key(),
            ),
        )
        .route(
            "/payment-reversals",
            with_permission(
                post(returns::payment_reversal_create),
                rbac,
                returns::payment_reversal_create_permission_key(),
            ),
        )
        .route(
            "/payment-reversals/{id}/post",
            with_permission(
                post(returns::payment_reversal_post),
                rbac,
                returns::payment_reversal_post_permission_key(),
            ),
        )
}
