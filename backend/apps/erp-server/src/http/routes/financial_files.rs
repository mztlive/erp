//! 财务文件按业务单据或任务详情权限受控下载。
use axum::Router;
use axum::routing::get;
use erp_permissions as permissions;

use crate::http::handler::financial_files;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 构建财务文件业务访问路由。
/// # 参数
/// 无。
/// # 返回
/// 绑定现有业务详情权限的路由集合。
/// # 错误
/// 无。
pub fn routes() -> Router<HttpState> {
    invoice_routes()
        .merge(receipt_routes())
        .route(
            "/work-items/{id}/purchase-payment",
            permission_route(
                get(financial_files::fulfillment_payment::purchase_payment_summary),
                &permissions::work_item::DETAIL,
            ),
        )
        .route(
            "/sales-orders/{id}/invoice-files",
            permission_route(get(financial_files::sales_invoice_files), &permissions::sales_order::DETAIL),
        )
        .route(
            "/sales-orders/{id}/invoice-files/{invoice_id}/{asset_id}/download",
            permission_route(get(financial_files::sales_invoice_download), &permissions::sales_order::DETAIL),
        )
        .route(
            "/work-items/{id}/payment-receipts",
            permission_route(
                get(financial_files::purchase_payment_receipts),
                &permissions::work_item::DETAIL,
            ),
        )
        .route(
            "/work-items/{id}/payment-receipts/{payment_id}/download",
            permission_route(
                get(financial_files::purchase_payment_receipt_download),
                &permissions::work_item::DETAIL,
            ),
        )
}

/// 按整票读取权限挂载发票文件清单与下载。
fn invoice_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/invoices/{id}/files",
            permission_route(
                get(financial_files::invoice::invoice_files),
                &permissions::receivable::INVOICE_DETAIL,
            ),
        )
        .route(
            "/invoices/{id}/files/{asset_id}/download",
            permission_route(
                get(financial_files::invoice::invoice_download),
                &permissions::receivable::INVOICE_DETAIL,
            ),
        )
}

/// 回款凭证查看与草稿编辑下载。
fn receipt_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-receipts/{id}/files",
            permission_route(
                get(financial_files::receipt::receipt_files),
                &permissions::receivable::CUSTOMER_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/customer-receipts/{id}/files/{asset_id}/download",
            permission_route(
                get(financial_files::receipt::receipt_file_download),
                &permissions::receivable::CUSTOMER_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/customer-receipts/{id}/draft-files",
            permission_route(
                get(financial_files::receipt::receipt_draft_files),
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
        .route(
            "/customer-receipts/{id}/draft-files/{asset_id}/download",
            permission_route(
                get(financial_files::receipt::receipt_draft_file_download),
                &permissions::receivable::CUSTOMER_RECEIPT_SUBMIT,
            ),
        )
}
