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
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    account_routes(rbac).merge(payment_routes(rbac)).merge(invoice_routes(rbac))
}

/// 应付子账读取、建立与收款账户查看。
fn account_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/payable-accounts",
            with_permission(get(payable::payable_account_list), rbac, permissions::payable::ACCOUNT_LIST),
        )
        .route(
            "/payable-accounts",
            with_permission(
                post(payable::payable_account_create),
                rbac,
                permissions::payable::ACCOUNT_CREATE,
            ),
        )
        .route(
            "/payable-accounts/{id}",
            with_permission(get(payable::payable_account_detail), rbac, permissions::payable::ACCOUNT_DETAIL),
        )
        .route(
            "/payable-accounts/{id}/payment-recipient/reveal",
            with_permission(
                post(payable::payment_recipient_reveal),
                rbac,
                permissions::payable::PAYMENT_RECIPIENT_REVEAL,
            ),
        )
}

/// 供应商付款读取、回单与提交。
fn payment_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-payments",
            with_permission(
                get(payable::supplier_payment_list),
                rbac,
                permissions::payable::SUPPLIER_PAYMENT_LIST,
            ),
        )
        .route(
            "/supplier-payments/merge-candidates",
            with_permission(
                get(payable::supplier_payment_merge_candidates),
                rbac,
                permissions::payable::SUPPLIER_PAYMENT_MERGE_CANDIDATES,
            ),
        )
        .route(
            "/supplier-payments/{id}",
            with_permission(
                get(payable::supplier_payment_detail),
                rbac,
                permissions::payable::SUPPLIER_PAYMENT_DETAIL,
            ),
        )
        .route(
            "/supplier-payments/{id}/bank-receipt",
            with_permission(
                get(payable::supplier_payment_bank_receipt),
                rbac,
                permissions::payable::SUPPLIER_PAYMENT_BANK_RECEIPT,
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
                permissions::payable::SUPPLIER_PAYMENT_COMMIT,
            ),
        )
}

/// 进项发票分配列表与含文件登记。
fn invoice_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-invoice-allocations",
            with_permission(
                get(payable::purchase_invoice_allocation_list),
                rbac,
                permissions::payable::PURCHASE_INVOICE_ALLOCATION_LIST,
            ),
        )
        .route(
            "/purchase-invoice-allocations",
            with_permission(
                post(payable::purchase_invoice_allocation_post),
                rbac,
                permissions::payable::PURCHASE_INVOICE_ALLOCATION_POST,
            ),
        )
        .route(
            "/purchase-invoice-allocations/with-files",
            with_permission(
                upload::multipart_route(
                    post(payable::invoice_files::purchase_invoice_allocation_post_with_files),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::payable::INVOICE_FILES_PURCHASE_INVOICE_ALLOCATION_POST_WITH_FILES,
            ),
        )
}
