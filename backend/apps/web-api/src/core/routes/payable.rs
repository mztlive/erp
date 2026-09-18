//! 域 D19 `payable` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/payable-accounts`、
//! `/admin/supplier-payments`、`/admin/purchase-invoice-allocations`；
//! 每条路由统一走 JWT + RBAC（`with_permission`）。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::payable;
use crate::core::middleware::with_permission;
use crate::core::upload;

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
            "/payable-accounts",
            with_permission(
                get(payable::payable_account_list),
                rbac,
                payable::payable_account_list_permission_key(),
            ),
        )
        .route(
            "/payable-accounts",
            with_permission(
                post(payable::payable_account_create),
                rbac,
                payable::payable_account_create_permission_key(),
            ),
        )
        .route(
            "/payable-accounts/{id}",
            with_permission(
                get(payable::payable_account_detail),
                rbac,
                payable::payable_account_detail_permission_key(),
            ),
        )
        .route(
            "/payable-accounts/{id}/payment-recipient/reveal",
            with_permission(
                post(payable::payment_recipient_reveal),
                rbac,
                payable::payment_recipient_reveal_permission_key(),
            ),
        )
        .route(
            "/supplier-payments",
            with_permission(
                get(payable::supplier_payment_list),
                rbac,
                payable::supplier_payment_list_permission_key(),
            ),
        )
        .route(
            "/supplier-payments/merge-candidates",
            with_permission(
                get(payable::supplier_payment_merge_candidates),
                rbac,
                payable::supplier_payment_merge_candidates_permission_key(),
            ),
        )
        .route(
            "/supplier-payments/{id}",
            with_permission(
                get(payable::supplier_payment_detail),
                rbac,
                payable::supplier_payment_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-payments/{id}/bank-receipt",
            with_permission(
                get(payable::supplier_payment_bank_receipt),
                rbac,
                payable::supplier_payment_bank_receipt_permission_key(),
            ),
        )
        .route(
            "/supplier-payments/commit",
            with_permission(
                upload::multipart_route(
                    post(payable::supplier_payment_commit),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                payable::supplier_payment_commit_permission_key(),
            ),
        )
        .route(
            "/purchase-invoice-allocations",
            with_permission(
                get(payable::purchase_invoice_allocation_list),
                rbac,
                payable::purchase_invoice_allocation_list_permission_key(),
            ),
        )
        .route(
            "/purchase-invoice-allocations",
            with_permission(
                post(payable::purchase_invoice_allocation_post),
                rbac,
                payable::purchase_invoice_allocation_post_permission_key(),
            ),
        )
}
