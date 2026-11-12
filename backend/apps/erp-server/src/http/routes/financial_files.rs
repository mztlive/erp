//! 财务文件按业务单据或任务详情权限受控下载。
use axum::Router;
use axum::routing::get;
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::financial_files;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;

/// 构建财务文件业务访问路由。
/// # 参数
/// 当前共享 RBAC 服务。
/// # 返回
/// 绑定现有业务详情权限的路由集合。
/// # 错误
/// 无。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    invoice_routes(rbac)
        .merge(receipt_routes(rbac))
        .route(
            "/work-items/{id}/purchase-payment",
            with_permission(
                get(financial_files::fulfillment_payment::purchase_payment_summary),
                rbac,
                &permissions::work_item::DETAIL,
            ),
        )
        .route(
            "/sales-orders/{id}/invoice-files",
            with_permission(
                get(financial_files::sales_invoice_files),
                rbac,
                &permissions::sales_order::DETAIL,
            ),
        )
        .route(
            "/sales-orders/{id}/invoice-files/{invoice_id}/{asset_id}/download",
            with_permission(
                get(financial_files::sales_invoice_download),
                rbac,
                &permissions::sales_order::DETAIL,
            ),
        )
        .route(
            "/work-items/{id}/payment-receipts",
            with_permission(
                get(financial_files::purchase_payment_receipts),
                rbac,
                &permissions::work_item::DETAIL,
            ),
        )
        .route(
            "/work-items/{id}/payment-receipts/{payment_id}/download",
            with_permission(
                get(financial_files::purchase_payment_receipt_download),
                rbac,
                &permissions::work_item::DETAIL,
            ),
        )
}

/// 按整票读取权限挂载发票文件清单与下载。
fn invoice_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/invoices/{id}/files",
            with_permission(
                get(financial_files::invoice::invoice_files),
                rbac,
                &permissions::receivable::INVOICE_DETAIL,
            ),
        )
        .route(
            "/invoices/{id}/files/{asset_id}/download",
            with_permission(
                get(financial_files::invoice::invoice_download),
                rbac,
                &permissions::receivable::INVOICE_DETAIL,
            ),
        )
}

/// 回款凭证查看与草稿编辑下载。
fn receipt_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-receipts/{id}/files",
            with_permission(
                get(financial_files::receipt::receipt_files),
                rbac,
                &permissions::receivable::CUSTOMER_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/customer-receipts/{id}/files/{asset_id}/download",
            with_permission(
                get(financial_files::receipt::receipt_file_download),
                rbac,
                &permissions::receivable::CUSTOMER_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/customer-receipts/{id}/draft-files",
            with_permission(
                get(financial_files::receipt::receipt_draft_files),
                rbac,
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
        .route(
            "/customer-receipts/{id}/draft-files/{asset_id}/download",
            with_permission(
                get(financial_files::receipt::receipt_draft_file_download),
                rbac,
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
}
