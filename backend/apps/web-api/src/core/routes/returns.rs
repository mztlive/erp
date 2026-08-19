//! 域 D21 `returns` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-return-cases`、
//! `/admin/purchase-return-orders`、`/admin/customer-refunds`、
//! `/admin/supplier-refunds`、`/admin/receipt-reversals`、`/admin/payment-reversals`；
//! 每条路由统一走 JWT + RBAC（`with_permission`）。

use axum::{
    Router,
    routing::{get, post},
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
            "/customer-refunds/{id}/submit",
            with_permission(
                post(returns::customer_refund_submit),
                rbac,
                returns::customer_refund_submit_permission_key(),
            ),
        )
        .route(
            "/customer-refunds/{id}/cancel-approval",
            with_permission(
                post(returns::customer_refund_cancel_approval),
                rbac,
                returns::customer_refund_cancel_approval_permission_key(),
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
            "/supplier-refunds/{id}",
            with_permission(
                get(returns::supplier_refund_detail),
                rbac,
                returns::supplier_refund_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-refunds/{id}/submit",
            with_permission(
                post(returns::supplier_refund_submit),
                rbac,
                returns::supplier_refund_submit_permission_key(),
            ),
        )
        .route(
            "/supplier-refunds/{id}/cancel-approval",
            with_permission(
                post(returns::supplier_refund_cancel_approval),
                rbac,
                returns::supplier_refund_cancel_approval_permission_key(),
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
            "/receipt-reversals/{id}",
            with_permission(
                get(returns::receipt_reversal_detail),
                rbac,
                returns::receipt_reversal_detail_permission_key(),
            ),
        )
        .route(
            "/receipt-reversals/{id}/submit",
            with_permission(
                post(returns::receipt_reversal_submit),
                rbac,
                returns::receipt_reversal_submit_permission_key(),
            ),
        )
        .route(
            "/receipt-reversals/{id}/cancel-approval",
            with_permission(
                post(returns::receipt_reversal_cancel_approval),
                rbac,
                returns::receipt_reversal_cancel_approval_permission_key(),
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

#[cfg(test)]
mod tests {
    /// 客户退款路由暴露提交与撤回，不再把过账当客户端旁路入口。
    #[test]
    fn customer_refund_routes_expose_submit_and_cancel() {
        let production = include_str!("returns.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产路由必须存在");
        assert!(production.contains("/customer-refunds/{id}/submit"));
        assert!(production.contains("/customer-refunds/{id}/cancel-approval"));
        assert!(production.contains("customer_refund_submit"));
        assert!(production.contains("customer_refund_cancel_approval"));
        assert!(!production.contains("PENDING_REVIEW"));
    }

    /// 供应商退款路由暴露提交与撤回，不再把过账当客户端旁路入口。
    #[test]
    fn supplier_refund_routes_expose_submit_and_cancel() {
        let production = include_str!("returns.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产路由必须存在");
        assert!(production.contains("/supplier-refunds/{id}/submit"));
        assert!(production.contains("/supplier-refunds/{id}/cancel-approval"));
        assert!(production.contains("supplier_refund_submit"));
        assert!(production.contains("supplier_refund_cancel_approval"));
        assert!(production.contains("supplier_refund_detail"));
        assert!(!production.contains("PENDING_REVIEW"));
    }

    /// 回款冲正路由暴露提交与撤回，不再把过账当客户端旁路入口。
    #[test]
    fn receipt_reversal_routes_expose_submit_and_cancel() {
        let production = include_str!("returns.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产路由必须存在");
        assert!(production.contains("/receipt-reversals/{id}/submit"));
        assert!(production.contains("/receipt-reversals/{id}/cancel-approval"));
        assert!(production.contains("receipt_reversal_submit"));
        assert!(production.contains("receipt_reversal_cancel_approval"));
        assert!(production.contains("receipt_reversal_detail"));
        assert!(!production.contains("PENDING_REVIEW"));
    }
}
