//! 域 D18 `receivable` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/receivable-accounts`、
//! `/admin/customer-receipts`、`/admin/invoices`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::http::handler::receivable;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::http::upload;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<HttpState> {
    receipt_file_routes(rbac)
        .merge(additional_document_routes(rbac))
        .route(
            "/receivable-accounts",
            with_permission(
                get(receivable::receivable_account_list),
                rbac,
                permissions::receivable::ACCOUNT_LIST,
            ),
        )
        .route(
            "/receivable-accounts",
            with_permission(
                post(receivable::receivable_account_create),
                rbac,
                permissions::receivable::ACCOUNT_CREATE,
            ),
        )
        .route(
            "/receivable-accounts/{id}",
            with_permission(
                get(receivable::receivable_account_detail),
                rbac,
                permissions::receivable::ACCOUNT_DETAIL,
            ),
        )
        .route(
            "/customer-receipts",
            with_permission(
                get(receivable::customer_receipt_list),
                rbac,
                permissions::receivable::CUSTOMER_RECEIPT_LIST,
            ),
        )
        .route(
            "/customer-receipts",
            with_permission(
                post(receivable::customer_receipt_create),
                rbac,
                permissions::receivable::CUSTOMER_RECEIPT_CREATE,
            ),
        )
        .route(
            "/customer-receipts/{id}",
            with_permission(
                get(receivable::customer_receipt_detail),
                rbac,
                permissions::receivable::CUSTOMER_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/customer-receipts/drafts",
            with_permission(
                post(receivable::draft_update::customer_receipt_save_draft),
                rbac,
                permissions::receivable::DRAFT_UPDATE_CUSTOMER_RECEIPT_SAVE_DRAFT,
            ),
        )
        .route(
            "/customer-receipts/commit",
            with_permission(
                post(receivable::customer_receipt_commit),
                rbac,
                permissions::receivable::CUSTOMER_RECEIPT_COMMIT,
            ),
        )
        .route(
            "/customer-receipts/{id}/submit",
            with_permission(
                post(receivable::customer_receipt_submit),
                rbac,
                permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
        .route(
            "/customer-receipts/{id}/cancel-approval",
            with_permission(
                post(receivable::customer_receipt_cancel_approval),
                rbac,
                permissions::receivable::CUSTOMER_RECEIPT_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/customer-receipts/{id}/post",
            with_permission(
                post(receivable::customer_receipt_post),
                rbac,
                permissions::receivable::CUSTOMER_RECEIPT_POST,
            ),
        )
        .route(
            "/invoices",
            with_permission(get(receivable::invoice_list), rbac, permissions::receivable::INVOICE_LIST),
        )
        .route(
            "/invoices",
            with_permission(post(receivable::invoice_create), rbac, permissions::receivable::INVOICE_CREATE),
        )
        .route(
            "/invoices/{id}",
            with_permission(get(receivable::invoice_detail), rbac, permissions::receivable::INVOICE_DETAIL),
        )
        .route(
            "/sales-invoice-requests",
            with_permission(
                get(receivable::invoice_request::invoice_request_list),
                rbac,
                permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST,
            ),
        )
        .route(
            "/sales-invoice-requests/submit",
            with_permission(
                post(receivable::invoice_request::invoice_request_submit),
                rbac,
                permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
            ),
        )
        .merge(invoice_request_edit_routes(rbac))
        .route(
            "/sales-invoice-requests/amounts/{id}",
            with_permission(
                get(receivable::invoice_request::invoice_request_amounts),
                rbac,
                permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_AMOUNTS,
            ),
        )
        .route(
            "/invoices/commit",
            with_permission(post(receivable::invoice_commit), rbac, permissions::receivable::INVOICE_COMMIT),
        )
        .route(
            "/invoices/{id}/post",
            with_permission(post(receivable::invoice_post), rbac, permissions::receivable::INVOICE_POST),
        )
        .route(
            "/invoices/{id}/red-issue",
            with_permission(
                post(receivable::invoice_red_issue),
                rbac,
                permissions::receivable::INVOICE_RED_ISSUE,
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
fn additional_document_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/invoices/with-files",
            with_permission(
                upload::multipart_route(
                    post(receivable::invoice_files::invoice_create_with_files),
                    32 * upload::MAX_UPLOAD_FILE_BYTES + 1024 * 1024,
                ),
                rbac,
                permissions::receivable::INVOICE_FILES_INVOICE_CREATE_WITH_FILES,
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
                permissions::receivable::INVOICE_FILES_INVOICE_COMMIT_WITH_FILES,
            ),
        )
        .route(
            "/invoices/complete-with-files",
            with_permission(
                upload::multipart_route(
                    post(receivable::invoice_files::invoice_complete_with_files),
                    32 * upload::MAX_UPLOAD_FILE_BYTES + 1024 * 1024,
                ),
                rbac,
                permissions::receivable::INVOICE_FILES_INVOICE_COMPLETE_WITH_FILES,
            ),
        )
        .route(
            "/customer-receipts/{id}",
            with_permission(
                put(receivable::draft_update::customer_receipt_update),
                rbac,
                permissions::receivable::DRAFT_UPDATE_CUSTOMER_RECEIPT_UPDATE,
            ),
        )
        .route(
            "/customer-receipts/{id}/draft",
            with_permission(
                get(receivable::draft_read::customer_receipt_draft),
                rbac,
                permissions::receivable::DRAFT_READ_CUSTOMER_RECEIPT_DRAFT,
            ),
        )
}

/// 回款草稿凭证沿用提交权限和统一上传限制。
fn receipt_file_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new().route(
        "/customer-receipts/drafts/with-files",
        with_permission(
            upload::multipart_route(
                post(receivable::receipt_files::customer_receipt_save_with_files),
                upload::MAX_MULTIPART_REQUEST_BYTES,
            ),
            rbac,
            permissions::receivable::RECEIPT_FILES_CUSTOMER_RECEIPT_SAVE_WITH_FILES,
        ),
    )
}

fn invoice_request_edit_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-invoice-requests/{id}",
            with_permission(
                get(receivable::invoice_request::invoice_request_detail),
                rbac,
                permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
            ),
        )
        .route(
            "/sales-invoice-requests/{id}/cancel",
            with_permission(
                post(receivable::invoice_request::invoice_request_cancel),
                rbac,
                permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_CANCEL,
            ),
        )
        .route(
            "/sales-invoice-requests/{id}",
            with_permission(
                put(receivable::invoice_request::invoice_request_update),
                rbac,
                permissions::receivable::INVOICE_REQUEST_UPDATE,
            ),
        )
}
