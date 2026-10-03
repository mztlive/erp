//! 财务文件按业务单据或任务详情权限受控下载。
use axum::Router;
use axum::routing::get;
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::financial_files;
use crate::core::middleware::with_permission;

/// 构建财务文件业务访问路由。
/// # 参数
/// 当前共享 RBAC 服务。
/// # 返回
/// 绑定现有业务详情权限的路由集合。
/// # 错误
/// 无。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-orders/{id}/invoice-files",
            with_permission(
                get(financial_files::sales_invoice_files),
                rbac,
                financial_files::sales_invoice_files_permission_key(),
            ),
        )
        .route(
            "/sales-orders/{id}/invoice-files/{invoice_id}/{asset_id}/download",
            with_permission(
                get(financial_files::sales_invoice_download),
                rbac,
                financial_files::sales_invoice_download_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/payment-receipts",
            with_permission(
                get(financial_files::purchase_payment_receipts),
                rbac,
                financial_files::purchase_payment_receipts_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/payment-receipts/{payment_id}/download",
            with_permission(
                get(financial_files::purchase_payment_receipt_download),
                rbac,
                financial_files::purchase_payment_receipt_download_permission_key(),
            ),
        )
}
