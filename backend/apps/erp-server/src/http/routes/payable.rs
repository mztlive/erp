//! 域 D19 `payable` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/payable-accounts`、
//! `/admin/supplier-payments`、`/admin/purchase-invoice-allocations`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::payable;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
use crate::http::upload;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回声明公共权限目录信息的路由集合。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    account_routes().merge(payment_routes()).merge(invoice_routes())
}

/// 应付子账读取、建立与收款账户查看。
fn account_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/payable-accounts",
            permission_route(get(payable::payable_account_list), &permissions::payable::ACCOUNT_LIST),
        )
        .route(
            "/payable-accounts",
            permission_route(post(payable::payable_account_create), &permissions::payable::ACCOUNT_CREATE),
        )
        .route(
            "/payable-accounts/{id}",
            permission_route(get(payable::payable_account_detail), &permissions::payable::ACCOUNT_DETAIL),
        )
        .route(
            "/payable-accounts/{id}/payment-recipient/reveal",
            permission_route(
                post(payable::payment_recipient_reveal),
                &permissions::payable::PAYMENT_RECIPIENT_REVEAL,
            ),
        )
}

/// 供应商付款读取、回单与提交。
fn payment_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-payments",
            permission_route(
                get(payable::supplier_payment_list),
                &permissions::payable::SUPPLIER_PAYMENT_LIST,
            ),
        )
        .route(
            "/supplier-payments/merge-candidates",
            permission_route(
                get(payable::supplier_payment_merge_candidates),
                &permissions::payable::ACCOUNT_DETAIL,
            ),
        )
        .route(
            "/supplier-payments/{id}",
            permission_route(
                get(payable::supplier_payment_detail),
                &permissions::payable::SUPPLIER_PAYMENT_DETAIL,
            ),
        )
        .route(
            "/supplier-payments/{id}/bank-receipt",
            permission_route(
                get(payable::supplier_payment_bank_receipt),
                &permissions::payable::SUPPLIER_PAYMENT_DETAIL,
            ),
        )
        .route(
            "/supplier-payments/commit",
            permission_route(
                upload::multipart_route(
                    post(payable::supplier_payment_commit),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::payable::SUPPLIER_PAYMENT_COMMIT,
            ),
        )
}

/// 进项发票分配列表与含文件登记。
fn invoice_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-invoice-allocations",
            permission_route(
                get(payable::purchase_invoice_allocation_list),
                &permissions::payable::PURCHASE_INVOICE_ALLOCATION_LIST,
            ),
        )
        .route(
            "/purchase-invoice-allocations",
            permission_route(
                post(payable::purchase_invoice_allocation_post),
                &permissions::payable::PURCHASE_INVOICE_ALLOCATION_POST,
            ),
        )
        .route(
            "/purchase-invoice-allocations/with-files",
            permission_route(
                upload::multipart_route(
                    post(payable::invoice_files::purchase_invoice_allocation_post_with_files),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::payable::PURCHASE_INVOICE_ALLOCATION_POST,
            ),
        )
}
