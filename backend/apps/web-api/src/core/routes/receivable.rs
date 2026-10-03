//! 域 D18 `receivable` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/receivable-accounts`、
//! `/admin/customer-receipts`、`/admin/invoices`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::receivable;
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
        .merge(additional_document_routes(rbac))
        .route(
            "/receivable-accounts",
            with_permission(
                get(receivable::receivable_account_list),
                rbac,
                receivable::receivable_account_list_permission_key(),
            ),
        )
        .route(
            "/receivable-accounts",
            with_permission(
                post(receivable::receivable_account_create),
                rbac,
                receivable::receivable_account_create_permission_key(),
            ),
        )
        .route(
            "/receivable-accounts/{id}",
            with_permission(
                get(receivable::receivable_account_detail),
                rbac,
                receivable::receivable_account_detail_permission_key(),
            ),
        )
        .route(
            "/customer-receipts",
            with_permission(
                get(receivable::customer_receipt_list),
                rbac,
                receivable::customer_receipt_list_permission_key(),
            ),
        )
        .route(
            "/customer-receipts",
            with_permission(
                post(receivable::customer_receipt_create),
                rbac,
                receivable::customer_receipt_create_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}",
            with_permission(
                get(receivable::customer_receipt_detail),
                rbac,
                receivable::customer_receipt_detail_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/commit",
            with_permission(
                post(receivable::customer_receipt_commit),
                rbac,
                receivable::customer_receipt_commit_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}/submit",
            with_permission(
                post(receivable::customer_receipt_submit),
                rbac,
                receivable::customer_receipt_submit_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}/cancel-approval",
            with_permission(
                post(receivable::customer_receipt_cancel_approval),
                rbac,
                receivable::customer_receipt_cancel_approval_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}/post",
            with_permission(
                post(receivable::customer_receipt_post),
                rbac,
                receivable::customer_receipt_post_permission_key(),
            ),
        )
        .route(
            "/invoices",
            with_permission(get(receivable::invoice_list), rbac, receivable::invoice_list_permission_key()),
        )
        .route(
            "/invoices",
            with_permission(
                post(receivable::invoice_create),
                rbac,
                receivable::invoice_create_permission_key(),
            ),
        )
        .route(
            "/invoices/{id}",
            with_permission(
                get(receivable::invoice_detail),
                rbac,
                receivable::invoice_detail_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests",
            with_permission(
                get(receivable::invoice_request::invoice_request_list),
                rbac,
                receivable::invoice_request::invoice_request_list_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/submit",
            with_permission(
                post(receivable::invoice_request::invoice_request_submit),
                rbac,
                receivable::invoice_request::invoice_request_submit_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/{id}",
            with_permission(
                get(receivable::invoice_request::invoice_request_detail),
                rbac,
                receivable::invoice_request::invoice_request_detail_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/{id}/cancel",
            with_permission(
                post(receivable::invoice_request::invoice_request_cancel),
                rbac,
                receivable::invoice_request::invoice_request_cancel_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/amounts/{id}",
            with_permission(
                get(receivable::invoice_request::invoice_request_amounts),
                rbac,
                receivable::invoice_request::invoice_request_amounts_permission_key(),
            ),
        )
        .route(
            "/invoices/commit",
            with_permission(
                post(receivable::invoice_commit),
                rbac,
                receivable::invoice_commit_permission_key(),
            ),
        )
        .route(
            "/invoices/{id}/post",
            with_permission(post(receivable::invoice_post), rbac, receivable::invoice_post_permission_key()),
        )
        .route(
            "/invoices/{id}/red-issue",
            with_permission(
                post(receivable::invoice_red_issue),
                rbac,
                receivable::invoice_red_issue_permission_key(),
            ),
        )
}

/// 发票文件上传路由。
///
/// # 参数
/// * `rbac` - 当前共享权限服务
/// # 返回
/// 返回逐操作保留权限校验和上传限制的路由集合。
/// # 错误
/// 无；业务错误由各处理器返回。
fn additional_document_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/invoices/with-files",
            with_permission(
                upload::multipart_route(
                    post(receivable::invoice_files::invoice_create_with_files),
                    32 * upload::MAX_UPLOAD_FILE_BYTES + 1024 * 1024,
                ),
                rbac,
                receivable::invoice_files::invoice_create_with_files_permission_key(),
            ),
        )
        .route(
            "/invoices/commit-with-files",
            with_permission(
                upload::multipart_route(
                    post(receivable::invoice_files::invoice_commit_with_files),
                    32 * upload::MAX_UPLOAD_FILE_BYTES + 1024 * 1024,
                ),
                rbac,
                receivable::invoice_files::invoice_commit_with_files_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}",
            with_permission(
                put(receivable::draft_update::customer_receipt_update),
                rbac,
                receivable::draft_update::customer_receipt_update_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}/draft",
            with_permission(
                get(receivable::draft_read::customer_receipt_draft),
                rbac,
                receivable::draft_read::customer_receipt_draft_permission_key(),
            ),
        )
}
