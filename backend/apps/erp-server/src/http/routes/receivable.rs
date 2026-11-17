//! 域 D18 `receivable` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/receivable-accounts`、
//! `/admin/customer-receipts`、`/admin/invoices`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::receivable;
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
    receipt_file_routes()
        .merge(additional_document_routes())
        .merge(invoice_request_edit_routes())
        .merge(account_routes())
        .merge(receipt_query_routes())
        .merge(receipt_command_routes())
        .merge(invoice_query_routes())
        .merge(invoice_command_routes())
}

/// 应收子账与财务来源资料保留逐路由公共权限目录信息。
fn account_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/receivable-accounts/sources/{kind}/{id}",
            permission_route(
                get(receivable::allocation_source::detail),
                &permissions::receivable::ACCOUNT_DETAIL,
            ),
        )
        .route(
            "/receivable-accounts",
            permission_route(
                get(receivable::receivable_account_list),
                &permissions::receivable::ACCOUNT_LIST,
            ),
        )
        .route(
            "/receivable-accounts",
            permission_route(
                post(receivable::receivable_account_create),
                &permissions::receivable::ACCOUNT_CREATE,
            ),
        )
        .route(
            "/receivable-accounts/{id}",
            permission_route(
                get(receivable::receivable_account_detail),
                &permissions::receivable::ACCOUNT_DETAIL,
            ),
        )
}

/// 回款列表、详情与草稿保留逐路由公共权限目录信息。
fn receipt_query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-receipts",
            permission_route(
                get(receivable::customer_receipt_list),
                &permissions::receivable::CUSTOMER_RECEIPT_LIST,
            ),
        )
        .route(
            "/customer-receipts",
            permission_route(
                post(receivable::customer_receipt_create),
                &permissions::receivable::CUSTOMER_RECEIPT_CREATE,
            ),
        )
        .route(
            "/customer-receipts/{id}",
            permission_route(
                get(receivable::customer_receipt_detail),
                &permissions::receivable::CUSTOMER_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/customer-receipts/drafts",
            permission_route(
                post(receivable::draft_update::customer_receipt_save_draft),
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
}

/// 回款正式办理命令保留逐路由公共权限目录信息。
fn receipt_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-receipts/commit",
            permission_route(
                post(receivable::customer_receipt_commit),
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
        .route(
            "/customer-receipts/{id}/submit",
            permission_route(
                post(receivable::customer_receipt_submit),
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
        .route(
            "/customer-receipts/{id}/cancel-approval",
            permission_route(
                post(receivable::customer_receipt_cancel_approval),
                &permissions::receivable::CUSTOMER_RECEIPT_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/customer-receipts/{id}/post",
            permission_route(
                post(receivable::customer_receipt_post),
                &permissions::receivable::CUSTOMER_RECEIPT_POST,
            ),
        )
}

/// 发票与开票申请查询保留逐路由公共权限目录信息。
fn invoice_query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/invoices",
            permission_route(get(receivable::invoice_list), &permissions::receivable::INVOICE_LIST),
        )
        .route(
            "/invoices",
            permission_route(post(receivable::invoice_create), &permissions::receivable::INVOICE_CREATE),
        )
        .route(
            "/invoices/{id}",
            permission_route(get(receivable::invoice_detail), &permissions::receivable::INVOICE_DETAIL),
        )
        .route(
            "/sales-invoice-requests",
            permission_route(
                get(receivable::invoice_request::invoice_request_list),
                &permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST,
            ),
        )
        .route(
            "/sales-invoice-requests/submit",
            permission_route(
                post(receivable::invoice_request::invoice_request_submit),
                &permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
            ),
        )
        .route(
            "/sales-invoice-requests/amounts/{id}",
            permission_route(
                get(receivable::invoice_request::invoice_request_amounts),
                &permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST,
            ),
        )
}

/// 发票登记与红票命令保留逐路由公共权限目录信息。
fn invoice_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/invoices/commit",
            permission_route(post(receivable::invoice_commit), &permissions::receivable::INVOICE_POST),
        )
        .route(
            "/invoices/{id}/post",
            permission_route(post(receivable::invoice_post), &permissions::receivable::INVOICE_POST),
        )
        .route(
            "/invoices/{id}/red-issue",
            permission_route(
                post(receivable::invoice_red_issue),
                &permissions::receivable::INVOICE_RED_ISSUE,
            ),
        )
}

/// 发票文件上传路由。
///
/// # 参数
/// 无。
/// # 返回
/// 返回逐操作声明权限目录信息并保留上传限制的路由集合。
/// # 错误
/// 无；业务错误由各处理器返回。
fn additional_document_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/invoices/with-files",
            permission_route(
                upload::multipart_route(
                    post(receivable::invoice_files::invoice_create_with_files),
                    32 * upload::MAX_UPLOAD_FILE_BYTES + 1024 * 1024,
                ),
                &permissions::receivable::INVOICE_CREATE,
            ),
        )
        .route(
            "/invoices/commit-with-files",
            permission_route(
                upload::multipart_route(
                    post(receivable::invoice_files::invoice_commit_with_files),
                    32 * upload::MAX_UPLOAD_FILE_BYTES + 1024 * 1024,
                ),
                &permissions::receivable::INVOICE_POST,
            ),
        )
        .route(
            "/invoices/complete-with-files",
            permission_route(
                upload::multipart_route(
                    post(receivable::invoice_files::invoice_complete_with_files),
                    32 * upload::MAX_UPLOAD_FILE_BYTES + 1024 * 1024,
                ),
                &permissions::receivable::INVOICE_POST,
            ),
        )
        .route(
            "/customer-receipts/{id}",
            permission_route(
                put(receivable::draft_update::customer_receipt_update),
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
        .route(
            "/customer-receipts/{id}/draft",
            permission_route(
                get(receivable::draft_read::customer_receipt_draft),
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
}

/// 回款草稿凭证沿用提交权限和统一上传限制。
fn receipt_file_routes() -> Router<HttpState> {
    Router::new().route(
        "/customer-receipts/drafts/with-files",
        permission_route(
            upload::multipart_route(
                post(receivable::receipt_files::customer_receipt_save_with_files),
                upload::MAX_MULTIPART_REQUEST_BYTES,
            ),
            &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
        ),
    )
}

fn invoice_request_edit_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-invoice-requests/{id}",
            permission_route(
                get(receivable::invoice_request::invoice_request_detail),
                &permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
            ),
        )
        .route(
            "/sales-invoice-requests/{id}/cancel",
            permission_route(
                post(receivable::invoice_request::invoice_request_cancel),
                &permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_CANCEL,
            ),
        )
        .route(
            "/sales-invoice-requests/{id}",
            permission_route(
                put(receivable::invoice_request::invoice_request_update),
                &permissions::receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
            ),
        )
}
